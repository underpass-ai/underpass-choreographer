{{/*
Choreographer chart helpers.
*/}}

{{- define "choreographer.name" -}}
{{- default .Chart.Name .Values.nameOverride | trunc 63 | trimSuffix "-" -}}
{{- end -}}

{{- define "choreographer.fullname" -}}
{{- if .Values.fullnameOverride -}}
{{- .Values.fullnameOverride | trunc 63 | trimSuffix "-" -}}
{{- else -}}
{{- $name := default .Chart.Name .Values.nameOverride -}}
{{- if contains $name .Release.Name -}}
{{- .Release.Name | trunc 63 | trimSuffix "-" -}}
{{- else -}}
{{- printf "%s-%s" .Release.Name $name | trunc 63 | trimSuffix "-" -}}
{{- end -}}
{{- end -}}
{{- end -}}

{{- define "choreographer.labels" -}}
app.kubernetes.io/name: {{ include "choreographer.name" . }}
app.kubernetes.io/instance: {{ .Release.Name }}
app.kubernetes.io/managed-by: {{ .Release.Service }}
app.kubernetes.io/version: {{ .Chart.AppVersion | quote }}
helm.sh/chart: {{ printf "%s-%s" .Chart.Name .Chart.Version | replace "+" "_" | trunc 63 | trimSuffix "-" }}
app.kubernetes.io/part-of: underpass
{{- end -}}

{{- define "choreographer.selectorLabels" -}}
app.kubernetes.io/name: {{ include "choreographer.name" . }}
app.kubernetes.io/instance: {{ .Release.Name }}
{{- end -}}

{{- define "choreographer.serviceAccountName" -}}
{{- if .Values.serviceAccount.create -}}
{{- default (include "choreographer.fullname" .) .Values.serviceAccount.name -}}
{{- else -}}
{{- default "default" .Values.serviceAccount.name -}}
{{- end -}}
{{- end -}}

{{/*
Image reference. Enforces an explicit `tag` or `digest` unless the
development.allowMutableImageTags escape hatch is set.
*/}}
{{- define "choreographer.image" -}}
{{- $repo := required "image.repository is required" .Values.image.repository -}}
{{- if .Values.image.digest -}}
{{ $repo }}@{{ .Values.image.digest }}
{{- else if .Values.image.tag -}}
{{- if and (eq .Values.image.tag "latest") (not .Values.development.allowMutableImageTags) -}}
{{- fail "image.tag=\"latest\" is a mutable reference; set image.tag or image.digest to a pinned reference, or enable development.allowMutableImageTags for non-production use" -}}
{{- end -}}
{{ $repo }}:{{ .Values.image.tag }}
{{- else -}}
{{- fail "set image.tag or image.digest to a pinned reference" -}}
{{- end -}}
{{- end -}}
