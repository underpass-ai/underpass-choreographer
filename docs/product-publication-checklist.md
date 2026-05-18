# Checklist De Usabilidad Y Publicación

Fecha de creación: 2026-05-18

Documento vivo para ejecutar el plan de
[`product-usability-publication-plan.md`](./product-usability-publication-plan.md).

Convención:

- `[ ]` pendiente
- `[x]` hecho
- Cada check completado debería añadir una referencia breve: PR, commit,
  comando validado, documento creado o evidencia reproducible.

## Regla De Producto

- [x] La documentación pública dice explícitamente que Choreographer es
  agnóstico e independiente. Referencia: este documento,
  `README.md`, `docs/index.md`, `docs/backlog.md` y
  `docs/stack-gap-analysis.md`.
- [x] PIR aparece solo como caso de estudio histórico o posible uso, no
  como dependencia. Referencia: `docs/pir-choreographer-integration-design.md`
  y `docs/index.md`.
- [x] KMP aparece solo como posible proveedor de contexto, no como
  dependencia. Referencia: `README.md`, `docs/stack-gap-analysis.md`
  y `docs/product-usability-publication-plan.md`.
- [x] Runtime aparece como adaptador de ejecución opcional, no como
  requisito para usar Choreographer. Referencia: `README.md`,
  `docs/product-usability-publication-plan.md` y
  `docs/operations/deploy-kubernetes.md`.
- [ ] Las claims públicas están respaldadas por tests, E2E, scripts o
  documentación operativa reproducible.

## Fase 1 - Camino Local Usable

- [ ] Crear `docs/operations/compose-e2e.md`.
- [ ] Documentar los 9 escenarios de `make e2e-compose`.
- [ ] Documentar `stub-runtime`.
- [ ] Documentar `stub-llm`.
- [ ] Documentar el Report schema usado por los escenarios positivos.
- [ ] Documentar los provider shapes OpenAI y vLLM usados en E2E.
- [ ] Añadir quickstart sin servicios externos:
  `CHOREO_NATS_ENABLED=false just run`.
- [ ] Añadir quickstart de demo completa: `make e2e-compose`.
- [ ] Añadir quickstart MCP fixture:
  `CHOREO_MCP_BACKEND=fixture choreo-mcp`.
- [ ] Añadir quickstart MCP live contra gRPC local.
- [ ] Añadir ejemplo de `CreateCouncil`.
- [ ] Añadir ejemplo de `RegisterAgent`.
- [ ] Añadir ejemplo de `RegisterContract`.
- [ ] Añadir ejemplo de `RunCouncilDecision`.
- [ ] Añadir ejemplo de `Orchestrate`.
- [ ] Verificar que un usuario nuevo puede obtener un resultado
  validado sin leer código Rust.

## Fase 2 - Separar Demos De Producción

- [ ] Añadir selector de escenarios al e2e-runner.
- [ ] Definir grupo `compose` para escenarios 1-9.
- [ ] Definir grupo `cluster-connectivity` para escenarios 1-4.
- [ ] Definir grupo `runtime-stub` para escenario 5.
- [ ] Definir grupo `structured-output` para escenarios 6-9.
- [ ] Actualizar `make e2e-compose` para usar el grupo completo.
- [ ] Actualizar Kubernetes Job para no ejecutar por defecto escenarios
  que requieren `stub.echo` o `stub-llm`.
- [ ] Actualizar `docs/operations/deploy-kubernetes.md` con el nuevo
  modo de smoke real.
- [ ] Verificar que Kubernetes smoke pasa contra un deploy real.
- [ ] Verificar que `make e2e-compose` sigue probando todo el stack con
  fixtures.
- [ ] Eliminar cualquier mención operativa a "fallos esperados" como
  estado aceptable.

## Fase 3 - Consumer Smoke Integrable

- [ ] Añadir modo positive-path a `choreo-consumer-smoke`.
- [ ] Permitir registrar un agent `openai` contra endpoint
  OpenAI-compatible.
- [ ] Permitir registrar un agent `vllm` contra endpoint
  OpenAI-compatible.
- [ ] Registrar el Report schema desde el smoke positivo.
- [ ] Ejecutar `RunCouncilDecision` en Strict mode desde el smoke
  positivo.
- [ ] Validar `report_payload_validates = Passed`.
- [ ] Mantener modo NoopAgent rejection-path.
- [ ] Documentar modo rejection-path.
- [ ] Documentar modo positive-path.
- [ ] Documentar NATS opcional.
- [ ] Documentar exit codes.
- [ ] Añadir ejemplo de CI para consumidores.
- [ ] Verificar que un consumidor puede comprobar API viva, rechazo de
  output inválido, aceptación de JSON válido y propagación de
  correlation/causation por NATS.

## Fase 4 - Publicación Operativa

- [ ] Completar guía Helm de instalación mínima.
- [ ] Completar guía Helm con embedded NATS.
- [ ] Completar guía Helm con Runtime executor opcional.
- [ ] Completar guía Helm con TLS/mTLS.
- [ ] Completar guía Helm con Postgres secret.
- [ ] Completar guía Helm con provider env secrets.
- [ ] Añadir `SECURITY.md`.
- [ ] Añadir `CHANGELOG.md`.
- [ ] Añadir `CONTRIBUTING.md`.
- [ ] Añadir matriz de soporte de Rust version.
- [ ] Añadir matriz de soporte de image tags.
- [ ] Añadir matriz de soporte de chart versions.
- [ ] Añadir matriz de soporte de providers.
- [ ] Añadir matriz de soporte de postura Kubernetes.
- [ ] Documentar rollback con ejemplo real.
- [ ] Documentar upgrade con ejemplo real.
- [ ] Verificar que un operador puede desplegar imagen pinneada, chart
  OCI, secret refs y smoke post-deploy.

## Fase 5 - Release Candidate

- [ ] `just check` pasa.
- [ ] `just helm-lint` pasa.
- [ ] `just integration` pasa.
- [ ] `make e2e-compose` pasa.
- [ ] Kubernetes smoke con escenarios seleccionados pasa.
- [ ] `make consumer-smoke` rejection-path pasa.
- [ ] consumer-smoke positive-path contra `stub-llm` o provider
  compatible pasa.
- [ ] `make e2e-provider-vllm` pasa si se anuncia vLLM real.
- [ ] `cargo publish --dry-run -p choreo-mcp-proto` pasa.
- [ ] `cargo publish --dry-run -p choreo-mcp` pasa.
- [ ] Dry run de build/push de imagen pasa.
- [ ] Dry run de build/push de chart pasa.
- [ ] Tag RC creado.
- [ ] Imagen GHCR publicada.
- [ ] Helm chart OCI publicado.
- [ ] `choreo-mcp` instalable desde registry.
- [ ] Release notes incluyen límites explícitos.

## Fase 6 - Public Beta

- [ ] Publicar como **Underpass Choreographer Public Beta**.
- [ ] Descripción pública: generic agent council coordination plane.
- [ ] Descripción pública: gRPC + MCP + Helm.
- [ ] Descripción pública: caller-supplied context.
- [ ] Descripción pública: Runtime executor opcional.
- [ ] Descripción pública: structured outputs via JSON Schema.
- [ ] Evitar claim "production-ready" sin downstream smoke real.
- [ ] Evitar claim "KMP-integrated" si Choreographer no consulta KMP.
- [ ] Evitar claim "durable eventing".
- [ ] Evitar claim "real-time deliberation streaming" para turnos
  internos.

## Definición De Usable

- [ ] Se puede instalar localmente.
- [ ] Se puede instalar por Helm.
- [ ] Se pueden crear o listar councils.
- [ ] Se puede registrar un agent.
- [ ] Se puede registrar un output contract.
- [ ] Se puede ejecutar `RunCouncilDecision`.
- [ ] Se recibe un resultado validado.
- [ ] Se puede conectar por MCP.
- [ ] Se puede ejecutar un smoke reproducible.

## Definición De Publicable

- [ ] Los límites están documentados sin contradicciones.
- [ ] El quickstart funciona desde cero.
- [ ] Los smoke tests no tienen "fallos esperados".
- [ ] La release produce imagen versionada.
- [ ] La release produce chart versionado.
- [ ] La release produce binario MCP versionado.
- [ ] Hay guía de seguridad.
- [ ] Hay guía de contribución.
- [ ] Hay changelog.
- [ ] Hay documentación de rollback.
- [ ] Las claims públicas están respaldadas por tests, E2E o docs de
  operación reproducibles.

## Notas De Avance

Añadir entradas breves aquí cuando se completen bloques grandes.

- 2026-05-18: checklist inicial creado a partir del plan de usabilidad
  y publicación.
