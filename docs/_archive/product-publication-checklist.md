# Checklist De Usabilidad Y Publicación

> **Archived 2026-07-05.** Execution checklist for the usability plan;
> phases 1-4 and 7 are done and verified in-repo. The open items (RC tag,
> GHCR image, OCI chart, `choreo-mcp` publish, public beta) live on in
> `docs/release.md` — this file is no longer tracked as living state and
> does not cover the observability epic (#102-#120).

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

- [x] Crear `docs/operations/compose-e2e.md`. Referencia:
  `docs/operations/compose-e2e.md`.
- [x] Documentar los 9 escenarios de `make e2e-compose`. Referencia:
  `docs/operations/compose-e2e.md`.
- [x] Documentar `stub-runtime`. Referencia:
  `docs/operations/compose-e2e.md`.
- [x] Documentar `stub-llm`. Referencia:
  `docs/operations/compose-e2e.md`.
- [x] Documentar el Report schema usado por los escenarios positivos.
  Referencia: `docs/operations/compose-e2e.md`.
- [x] Documentar los provider shapes OpenAI y vLLM usados en E2E.
  Referencia: `docs/operations/compose-e2e.md`.
- [x] Añadir quickstart sin servicios externos:
  `CHOREO_NATS_ENABLED=false just run`. Referencia: `README.md` y
  `docs/dev-loop.md`.
- [x] Añadir quickstart de demo completa: `make e2e-compose`.
  Referencia: `docs/operations/compose-e2e.md`.
- [x] Añadir quickstart MCP fixture:
  `CHOREO_MCP_BACKEND=fixture choreo-mcp`. Referencia:
  `docs/operations/mcp-stdio.md`, `README.md` y `docs/dev-loop.md`.
- [x] Añadir quickstart MCP live contra gRPC local. Referencia:
  `docs/operations/mcp-stdio.md`, `README.md` y `docs/dev-loop.md`.
- [x] Añadir ejemplo de `CreateCouncil`. Referencia:
  `docs/operations/mcp-stdio.md`.
- [x] Añadir ejemplo de `RegisterAgent`. Referencia:
  `docs/operations/mcp-stdio.md`.
- [x] Añadir ejemplo de `RegisterContract`. Referencia:
  `docs/operations/mcp-stdio.md`.
- [x] Añadir ejemplo de `RunCouncilDecision`. Referencia:
  `docs/operations/mcp-stdio.md`.
- [x] Añadir ejemplo de `Orchestrate`. Referencia:
  `docs/operations/mcp-stdio.md`.
- [x] Verificar que un usuario nuevo puede obtener un resultado
  validado sin leer código Rust. Evidencia:
  `bash scripts/ci/e2e-compose.sh` pasó el 2026-05-18; escenario 8
  ejecutó `RunCouncilDecision` en Strict contra el contrato Report con
  `candidates_passed=1` y `candidates_total=1`.

## Fase 2 - Separar Demos De Producción

- [x] Añadir selector de escenarios al e2e-runner. Referencia:
  `crates/choreo-e2e-runner/src/scenario_selection.rs`.
- [x] Definir grupo `compose` para escenarios 1-9. Referencia:
  `crates/choreo-e2e-runner/src/scenario_selection.rs`.
- [x] Definir grupo `cluster-connectivity` para escenarios 1-4.
  Referencia: `crates/choreo-e2e-runner/src/scenario_selection.rs`.
- [x] Definir grupo `runtime-stub` para escenario 5. Referencia:
  `crates/choreo-e2e-runner/src/scenario_selection.rs`.
- [x] Definir grupo `structured-output` para escenarios 6-9.
  Referencia: `crates/choreo-e2e-runner/src/scenario_selection.rs`.
- [x] Actualizar `make e2e-compose` para usar el grupo completo.
  Referencia: `tests/e2e/docker-compose.e2e.yaml`.
- [x] Actualizar Kubernetes Job para no ejecutar por defecto escenarios
  que requieren `stub.echo` o `stub-llm`. Referencia:
  `tests/e2e/kubernetes/runner-job.yaml`.
- [x] Actualizar `docs/operations/deploy-kubernetes.md` con el nuevo
  modo de smoke real. Referencia:
  `docs/operations/deploy-kubernetes.md`.
- [x] Verificar que Kubernetes smoke pasa contra un deploy real.
  Validado contra `underpass-runtime` con
  `CHOREO_E2E_SCENARIOS=cluster-connectivity`; el runner selecciona
  escenarios 1-4 y termina con `E2E scenarios passed`.
- [x] Verificar que `make e2e-compose` sigue probando todo el stack con
  fixtures. Validado con `CONTAINER_RUNTIME=podman-compose make
  e2e-compose`; el runner selecciona escenarios 1-9 y termina con
  `E2E scenarios passed`.
- [x] Eliminar cualquier mención operativa a "fallos esperados" como
  estado aceptable.

## Fase 2.1 - Ceremonia E2E vLLM Real Multiagente

Objetivo: demostrar una ceremonia peer-review 360 real con varios
agentes `vllm` contra un endpoint vLLM externo. Esto no convierte vLLM
en dependencia del producto; es una validación de provider externo y
Choreographer sigue siendo agnóstico.

- [x] Inventariar cobertura existente: `make e2e-provider-vllm` prueba
  el adapter `agent-vllm` contra vLLM real, pero no arranca
  Choreographer ni crea council. Referencia:
  `crates/choreo-e2e-runner/src/bin/provider.rs`.
- [x] Inventariar cobertura existente: escenario 9 de
  `make e2e-compose` prueba `kind=vllm` a través de Choreographer, pero
  usa `stub-llm` y un solo agente. Referencia:
  `crates/choreo-e2e-runner/src/scenarios/structured_output/report_vllm.rs`.
- [x] Validar endpoint vLLM real accesible con mTLS. Validado el
  2026-05-18 contra `https://vllm.underpassai.com` con modelo
  `google/gemma-4-31B-it`: `/health`, `/v1/models` y
  `/v1/chat/completions` respondieron correctamente.
- [x] Validar `make e2e-provider-vllm` contra vLLM real. Validado el
  2026-05-18 con imagen
  `ghcr.io/underpass-ai/underpass-choreographer-e2e-provider:sha-c74aac2`,
  `E2E_CLIENT_TLS_SECRET=underpass-demo-client-tls`,
  `E2E_IMAGE_PULL_SECRET=ghcr-pull`,
  `CHOREO_VLLM_ENDPOINT=https://vllm.underpassai.com` y
  `CHOREO_VLLM_MODEL=google/gemma-4-31B-it`; el Job terminó con
  `Job completed successfully`.
- [x] Añadir escenario E2E que arranque contra un Choreographer real y
  registre varios agentes `kind=vllm` apuntando al endpoint vLLM
  externo. Referencia:
  `crates/choreo-e2e-runner/src/scenarios/structured_output/real_vllm_multi_agent.rs`.
- [x] Crear council con `num_agents > 1` y ejecutar
  `RunCouncilDecision` en `Strict` contra el contrato Report.
  Referencia:
  `crates/choreo-e2e-runner/src/scenarios/structured_output/real_vllm_multi_agent.rs`.
- [x] Exponer evidencia de interacción entre agentes en la respuesta
  pública: `Proposal.revision_count` y
  `CandidateSummary.revision_count`. Referencia:
  `crates/choreo-proto/proto/underpass/choreo/v1/choreo.proto`.
- [x] Asertar que el resultado viene de una ceremonia multiagente:
  `candidates_total > 1`, autores distintos, `revision_count > 0` en
  winner y candidatos, `candidates_passed >= 1`, winner presente y
  payload válido contra JSON Schema. Referencia:
  `crates/choreo-e2e-runner/src/scenarios/structured_output/real_vllm_multi_agent.rs`.
- [x] Añadir Job/Make target separado para este flujo:
  `make e2e-council-vllm`. Referencia:
  `scripts/ci/e2e-council-vllm.sh`,
  `tests/e2e/kubernetes/council-vllm-job.yaml` y `Makefile`.
- [x] Validar compilación y tests locales del cambio. Validado con
  `cargo check -p choreo-e2e-runner -p choreo-adapters -p choreo-mcp
  --locked --features choreo-adapters/agent-vllm` y
  `cargo test -p choreo-e2e-runner -p choreo-adapters -p choreo-mcp
  --locked --features choreo-adapters/agent-vllm`.
- [ ] Build/push de imagen E2E runner con este escenario, habilitar
  `kind=vllm` en el Choreographer objetivo mediante
  `CHOREO_VLLM_MODEL` + `CHOREO_VLLM_ENDPOINT`, y ejecutar
  `make e2e-council-vllm` contra `vllm.underpassai.com`.
- [ ] Documentar operación, requisitos y límites del E2E multiagente
  vLLM en `docs/operations/compose-e2e.md`, `docs/dev-loop.md` y
  `docs/operations/support-matrix.md`.

## Fase 2.2 - Ceremonia E2E MCP

Objetivo: probar la misma ceremonia desde la superficie MCP, no solo
desde gRPC. MCP debe demostrar paridad de ejecución: registrar contrato,
registrar agentes, crear council, ejecutar decisión y recibir evidencia
de interacción (`revision_count`) en la respuesta.

- [x] Añadir test/runner MCP live contra Choreographer real usando
  `choreo-mcp` como backend stdio. Referencia:
  `scripts/mcp/choreo-mcp-council-vllm.py`.
- [x] Ejecutar por MCP `choreo_register_contract`,
  `choreo_register_agent`, `choreo_create_council` y
  `choreo_run_council_decision`. Referencia:
  `scripts/mcp/choreo-mcp-council-vllm.py`.
- [x] Asertar desde la respuesta MCP: `candidates_total > 1`,
  `candidates_passed >= 1`, winner JSON válido y
  `revision_count > 0`. Referencia:
  `scripts/mcp/choreo-mcp-council-vllm.py`.
- [x] Añadir Make/script separado:
  `make e2e-mcp-council-vllm`, para no mezclarlo con el runner gRPC.
  Referencia: `Makefile` y `scripts/ci/e2e-mcp-council-vllm.sh`.
- [x] Documentar el flujo MCP live y su relación con el E2E gRPC.
  Referencia: `docs/operations/mcp-stdio.md`,
  `docs/operations/compose-e2e.md` y `docs/dev-loop.md`.
- [ ] Ejecutar `make e2e-mcp-council-vllm` contra Choreographer +
  vLLM real y registrar la evidencia del Job/smoke.

## Fase 3 - Consumer Smoke Integrable

- [x] Añadir modo positive-path a `choreo-consumer-smoke`.
  Referencia: `crates/choreo-consumer-smoke/src/positive.rs`.
- [x] Permitir registrar un agent `openai` contra endpoint
  OpenAI-compatible. Referencia:
  `crates/choreo-consumer-smoke/src/positive.rs`.
- [x] Permitir registrar un agent `vllm` contra endpoint
  OpenAI-compatible. Referencia:
  `crates/choreo-consumer-smoke/src/positive.rs`.
- [x] Registrar el Report schema desde el smoke positivo. Referencia:
  `crates/choreo-consumer-smoke/src/positive.rs`.
- [x] Ejecutar `RunCouncilDecision` en Strict mode desde el smoke
  positivo. Validado con `--chain positive-path` contra
  `choreo-stub-llm` local.
- [x] Validar `report_payload_validates = Passed`. Validado con
  `--chain positive-path` contra `choreo-stub-llm` local.
- [x] Mantener modo NoopAgent rejection-path. Referencia:
  `crates/choreo-consumer-smoke/src/chain2.rs`.
- [x] Documentar modo rejection-path. Referencia:
  `docs/operations/consumer-smoke.md`.
- [x] Documentar modo positive-path. Referencia:
  `docs/operations/consumer-smoke.md`.
- [x] Documentar NATS opcional. Referencia:
  `docs/operations/consumer-smoke.md`.
- [x] Documentar exit codes. Referencia:
  `docs/operations/consumer-smoke.md`.
- [x] Añadir ejemplo de CI para consumidores. Referencia:
  `docs/operations/consumer-smoke.md`.
- [x] Verificar que un consumidor puede comprobar API viva, rechazo de
  output inválido, aceptación de JSON válido y propagación de
  correlation/causation por NATS. Validado con
  `cargo run -p choreo-consumer-smoke --locked -- --endpoint
  http://127.0.0.1:58055 --nats-url nats://127.0.0.1:58222 --chain
  all` y `cargo run -p choreo-consumer-smoke --locked -- --endpoint
  http://127.0.0.1:58055 --nats-url nats://127.0.0.1:58222 --chain
  positive-path --provider-kind openai --provider-endpoint
  http://127.0.0.1:58000 --provider-model stub-report-v1` contra
  Choreographer local, NATS y `choreo-stub-llm`.

## Fase 4 - Publicación Operativa

- [x] Completar guía Helm de instalación mínima. Referencia:
  `docs/operations/deploy-kubernetes.md` y
  `charts/choreographer/values.minimal.yaml`.
- [x] Completar guía Helm con embedded NATS. Referencia:
  `docs/operations/deploy-kubernetes.md` y
  `charts/choreographer/values.embedded-nats.yaml`.
- [x] Completar guía Helm con Runtime executor opcional. Referencia:
  `docs/operations/deploy-kubernetes.md`,
  `charts/choreographer/values.underpass-runtime.yaml` y
  `scripts/ci/helm-lint.sh`.
- [x] Completar guía Helm con TLS/mTLS. Referencia:
  `docs/operations/deploy-kubernetes.md` y
  `scripts/ci/helm-lint.sh`.
- [x] Completar guía Helm con Postgres secret. Referencia:
  `docs/operations/deploy-kubernetes.md`,
  `charts/choreographer/values.postgres-secret.yaml` y
  `scripts/ci/helm-lint.sh`.
- [x] Completar guía Helm con provider env secrets. Referencia:
  `docs/operations/deploy-kubernetes.md`,
  `charts/choreographer/values.provider-env-secrets.yaml` y
  `scripts/ci/helm-lint.sh`.
- [x] Añadir `SECURITY.md`. Referencia: `SECURITY.md`, `README.md` y
  `docs/index.md`.
- [x] Añadir `CHANGELOG.md`. Referencia: `CHANGELOG.md`, `README.md`
  y `docs/index.md`.
- [x] Añadir `CONTRIBUTING.md`. Referencia: `CONTRIBUTING.md`,
  `README.md` y `docs/index.md`.
- [x] Añadir matriz de soporte de Rust version. Referencia:
  `docs/operations/support-matrix.md`, `Cargo.toml`,
  `rust-toolchain.toml` y `.github/workflows/quality-gate.yml`.
- [x] Añadir matriz de soporte de image tags. Referencia:
  `docs/operations/support-matrix.md`,
  `.github/workflows/publish-distribution.yml` y
  `charts/choreographer/templates/_helpers.tpl`.
- [x] Añadir matriz de soporte de chart versions. Referencia:
  `docs/operations/support-matrix.md`,
  `charts/choreographer/Chart.yaml` y `scripts/release.sh`.
- [x] Añadir matriz de soporte de providers. Referencia:
  `docs/operations/support-matrix.md`,
  `crates/choreo-adapters/src/agents/factory.rs`, `Dockerfile` y
  `scripts/ci/quality-gate.sh`.
- [x] Añadir matriz de soporte de postura Kubernetes. Referencia:
  `docs/operations/support-matrix.md`,
  `docs/operations/deploy-kubernetes.md` y
  `scripts/ci/helm-lint.sh`.
- [x] Documentar rollback con ejemplo real. Referencia:
  `docs/operations/deploy-kubernetes.md`.
- [x] Documentar upgrade con ejemplo real. Referencia:
  `docs/operations/deploy-kubernetes.md`.
- [ ] Verificar que un operador puede desplegar imagen pinneada, chart
  OCI, secret refs y smoke post-deploy. Pendiente de tag `v*` y chart
  OCI publicado. Preparado y validado parcialmente el 2026-05-18:
  `helm template` con digest + Postgres/provider Secret refs,
  `helm template` con mTLS Secret ref y `helm package` local generando
  `choreographer-0.1.0.tgz`; runbook OCI post-release en
  `docs/operations/deploy-kubernetes.md`.

## Fase 5 - Release Candidate

- [x] `just check` pasa. Validado el 2026-05-18 con el equivalente
  directo `bash scripts/ci/quality-gate.sh`: contract gate, fmt-check,
  clippy con matriz de providers, tests workspace y bench-compile.
- [x] `just helm-lint` pasa. Validado el 2026-05-18 con
  `bash scripts/ci/helm-lint.sh`.
- [x] `just integration` pasa. Validado el 2026-05-18 con
  `bash scripts/ci/integration-nats.sh` y
  `bash scripts/ci/integration-postgres.sh` contra contenedores reales.
- [x] `make e2e-compose` pasa. Validado el 2026-05-18: build de
  servicios con Docker Compose, escenarios 1-9 y cierre con
  `E2E scenarios passed`.
- [x] Kubernetes smoke con escenarios seleccionados pasa. Validado el
  2026-05-18 con `make e2e-kubernetes`,
  `E2E_IMAGE_REPOSITORY_PREFIX=ghcr.io/underpass-ai`,
  `E2E_IMAGE_TAG=sha-c74aac2` y `E2E_IMAGE_PULL_SECRET=ghcr-pull`:
  escenarios 1-4 y cierre con `E2E kubernetes scenarios passed`.
- [x] `make consumer-smoke` rejection-path pasa. Validado el
  2026-05-18 contra RC temporal `sha-c74aac2` vía port-forward:
  `CONSUMER_SMOKE_CHAIN=two`, `report_schema_registered` PASS,
  `report_contract_rejects_freeform_text` PASS.
- [x] consumer-smoke positive-path contra `stub-llm` o provider
  compatible pasa. Validado el 2026-05-18 contra RC temporal
  `sha-c74aac2` con `stub-llm` en Kubernetes:
  `CONSUMER_SMOKE_CHAIN=positive-path`,
  `CONSUMER_SMOKE_PROVIDER_KIND=openai`,
  `report_payload_validates` PASS y
  `report_validation_summary_passed` PASS.
- [x] `make e2e-provider-vllm` pasa contra vLLM real. Validado el
  2026-05-18 con runner
  `ghcr.io/underpass-ai/underpass-choreographer-e2e-provider:sha-c74aac2`,
  `E2E_CLIENT_TLS_SECRET=underpass-demo-client-tls`,
  `E2E_IMAGE_PULL_SECRET=ghcr-pull`,
  `CHOREO_VLLM_ENDPOINT=https://vllm.underpassai.com` y
  `CHOREO_VLLM_MODEL=google/gemma-4-31B-it`; el Job completó
  correctamente. El council multiagente con vLLM real queda como check
  separado en la Fase 2.1.
- [x] `cargo publish --dry-run -p choreo-mcp-proto` pasa. Validado el
  2026-05-18 en la rama `feat/bundle-b-mcp-distribution`; el crate
  vendorizado evita publicar el crate interno `choreo-proto`.
- [ ] `cargo publish --dry-run -p choreo-mcp` pasa. Bloqueado hasta
  publicar primero `choreo-mcp-proto v0.1.0` en crates.io: el
  manifest ya apunta al crate vendorizado y `cargo check -p
  choreo-mcp` pasa, pero el dry-run completo resuelve
  `choreo-mcp-proto` contra crates.io y falla porque aún no existe
  allí.
- [x] Dry run de build/push de imagen pasa. Validado el 2026-05-18
  con Podman login contra GHCR usando token local, build y push de
  `ghcr.io/underpass-ai/underpass-choreographer:sha-c74aac2` y
  `ghcr.io/underpass-ai/underpass-choreographer-e2e-runner:sha-c74aac2`;
  para positive-path se publicó también
  `ghcr.io/underpass-ai/underpass-choreographer-stub-llm:sha-c74aac2`.
- [x] Dry run de build/push de chart pasa. Validado el 2026-05-18:
  `helm package charts/choreographer --destination /tmp/choreographer-chart-dry-run`,
  `helm lint charts/choreographer` y `helm registry login ghcr.io`
  pasaron. `helm push` no tiene modo dry-run; el push OCI real queda
  reservado para el check de chart publicado tras tag.
- [ ] Tag RC creado.
- [ ] Imagen GHCR publicada.
- [ ] Helm chart OCI publicado.
- [ ] `choreo-mcp` instalable desde registry.
- [x] Release notes incluyen límites explícitos. Validado el
  2026-05-18 en `CHANGELOG.md` con sección `Known Limits`.

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

## Fase 7 - Validación Juez Y Motor De Ceremonias

Cubre las dos capacidades que llegaron a `main` el 2026-06-06 → 2026-06-09:
el motor de ceremonias declarativo (YAML FSM) y el scorer LLM-as-judge.

- [x] Motor de ceremonias declarativo: `RunCeremony` ejecuta una FSM YAML
  (states/steps/transitions/guards/roles) con handlers enchufables.
  Referencia: `crates/choreo-app/src/usecases/run_ceremony_use_case.rs`,
  `crates/choreo-core/src/entities/ceremony_definition.rs`.
- [x] Paneles multiagente por paso (`num_agents`) y brief de contexto
  inyectado en la tarea de cada agente. Referencia:
  `crates/choreo-adapters/src/ceremony/deliberating_ceremony_step_handler.rs`.
- [x] Ceremonias del catálogo ejecutan E2E (daily standup, debate técnico,
  sprint planning, speaker + Q&A). Referencia:
  `tests/e2e/ceremonies/*.yaml`, `docs/operations/compose-e2e.md`
  (escenarios 11-16), bin `choreo-run-ceremony`.
- [x] Diagrama Mermaid de la conversación en la respuesta de `RunCeremony`.
- [x] Scorer LLM-as-judge: `JudgeAwareScoring` rankea por calidad
  intrínseca en vez de fracción de validadores que pasan. Referencia:
  `crates/choreo-adapters/src/scoring.rs`,
  `crates/choreo-adapters/src/agents/judge.rs`.
- [x] Juez opt-in y fail-fast (`CHOREO_JUDGE_ENABLED`,
  `CHOREO_JUDGE_THRESHOLD`): si se activa sin endpoint/modelo vLLM, no
  arranca. Referencia: `crates/choreo/src/compose.rs`,
  `crates/choreo-adapters/src/agents/mod.rs`.
- [x] Juez persistido en el overlay Helm de `underpass-runtime` con guard
  de chart y marker de CI. Referencia:
  `charts/choreographer/values.underpass-runtime.yaml`,
  `charts/choreographer/templates/deployment.yaml`,
  `scripts/ci/helm-lint.sh`.
- [x] Exponer `RunCeremony` como tool MCP `choreo_run_ceremony`.
  Referencia: `crates/choreo-mcp/src/grpc/tools.rs`.
- [ ] Validar el juez contra un vLLM real en un deploy de producto (modelo,
  credenciales, presupuesto de latencia, network policy).

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
- 2026-05-18: `docs/operations/compose-e2e.md` añadido como guía
  operativa para `make e2e-compose`; cubre los 9 escenarios,
  `stub-runtime`, `stub-llm`, Report schema y provider shapes
  OpenAI/vLLM.
- 2026-05-18: quickstart sin servicios externos elevado a `README.md`
  y `docs/dev-loop.md`: `CHOREO_NATS_ENABLED=false just run`.
- 2026-05-18: quickstart MCP fixture añadido a
  `docs/operations/mcp-stdio.md`, `README.md` y `docs/dev-loop.md`.
- 2026-05-18: quickstart MCP live local añadido: Choreographer con
  `CHOREO_NATS_ENABLED=false CHOREO_SEED_SPECIALTIES=triage just run`
  y MCP con `CHOREO_MCP_GRPC_ENDPOINT=http://127.0.0.1:50055`.
  Validado con fallback `cargo run --locked -p choreo` porque `just`
  no está instalado en este entorno; smoke live pasó con
  `CHOREO_MCP_GRPC_ENDPOINT=http://127.0.0.1:50055
  CHOREO_MCP_BIN=target/debug/choreo-mcp
  bash scripts/mcp/choreo-stdio-smoke.sh`.
- 2026-05-18: ejemplo MCP `CreateCouncil` añadido y validado en
  fixture mode con `choreo_create_council`.
- 2026-05-18: paridad MCP == gRPC reforzada con test en
  `crates/choreo-mcp/src/protocol.rs`: lee `choreo.proto`, deriva
  los nombres `choreo_*` esperados y comprueba catálogo, dispatch
  gRPC y fixture.
- 2026-05-18: ejemplo MCP `RegisterAgent` añadido y validado en
  fixture mode con `choreo_register_agent`.
- 2026-05-18: ejemplo MCP `RegisterContract` añadido y validado en
  fixture mode con `choreo_register_contract`.
- 2026-05-18: ejemplo MCP `RunCouncilDecision` añadido y validado en
  fixture mode con `choreo_run_council_decision`.
- 2026-05-18: ejemplo MCP `Orchestrate` añadido y validado en fixture
  mode con `choreo_orchestrate`.
- 2026-05-18: `bash scripts/ci/e2e-compose.sh` ejecutado completo.
  Escenario 8 produjo `RunCouncilDecision succeeded with Report-shaped
  winner` con `candidates_passed=1` y `candidates_total=1`; escenario
  9 repitió el camino positivo por el adapter `vllm`; el runner cerró
  con `E2E scenarios passed`.
- 2026-05-18: selector de escenarios añadido al `choreo-e2e-runner`
  mediante `CHOREO_E2E_SCENARIOS`; grupos definidos: `compose` / `all`
  (1-9), `cluster-connectivity` (1-4), `runtime-stub` (5) y
  `structured-output` (6-9). `make e2e-compose` usa `compose` por
  defecto y permite override desde el entorno.
- 2026-05-18: `choreo-e2e-runner` separado en módulos: `main.rs`
  conserva el dispatch, `scenario_selection.rs` conserva el parser y
  `scenarios/` divide las assertions por superficie.
- 2026-05-18: `SECURITY.md` añadido con alcance soportado, canal
  privado de reporte, disclosure coordinado, baseline operativa y
  contención de secretos; enlazado desde `README.md` y `docs/index.md`.
- 2026-05-18: `CHANGELOG.md` añadido en modo pre-release: mantiene
  `Unreleased`, declara `0.1.0` como pendiente de tag y enlaza la
  disciplina de release notes desde `README.md` y `docs/index.md`.
- 2026-05-18: `CONTRIBUTING.md` añadido con alcance del repo, setup,
  workflow de PR, gates obligatorios, cambios de contrato, expectativas
  de tests/docs, reglas de seguridad y flujo de release.
- 2026-05-18: matriz de soporte Rust añadida en
  `docs/operations/support-matrix.md`: soporte exacto `1.90.0`,
  `Cargo.lock` obligatorio y regla de cambio coordinada para toolchain,
  CI, docs y changelog.
- 2026-05-18: matriz de soporte de image tags añadida: digest como
  referencia preferida, `vX.Y.Z` tras tag, `sha-<short>` para smoke/RC,
  y `main`/`latest`/tags locales como referencias no productivas.
- 2026-05-18: matriz de soporte de chart versions añadida: chart
  `0.1.0` pendiente de tag, OCI chart versionado como `X.Y.Z`, lockstep
  obligatorio con Cargo/appVersion/imagen y sin referencias movibles.
- 2026-05-18: matriz de soporte de providers añadida: `noop` siempre,
  `openai` y `vllm` en imagen por defecto con env/feature gates,
  `anthropic` como feature probada en CI pero fuera del Dockerfile por
  defecto, y credenciales fuera de descriptores.
- 2026-05-18: matriz de postura Kubernetes añadida: perfiles mínimos,
  embedded NATS, Postgres secret, provider env, TLS/mTLS, Runtime,
  NetworkPolicy/PDB opt-in y límites explícitos como Ingress no
  gestionado por el chart y multi-réplica pendiente de state plan.
- 2026-05-18: rollback operativo documentado con ejemplo concreto:
  `helm history`, rollback de revisión `7` a `6`, verificación de
  rollout, imagen restaurada, health checks y consumer smoke posterior.
- 2026-05-18: upgrade operativo documentado con ejemplo concreto:
  render previo, `helm upgrade --install` desde checkout, variante OCI
  versionada, imagen por digest, `--atomic`, smoke posterior y checks
  de Secrets/Postgres/TLS/Runtime.
- 2026-05-18: primer gate de RC pasado con
  `bash scripts/ci/quality-gate.sh`: contrato, formato, clippy,
  tests workspace y bench compile. Se eliminó ruido de warnings en
  `crates/choreo-adapters/src/agents/factory.rs` para que
  bench-compile cierre limpio.
- 2026-05-18: gate Helm de RC pasado con
  `bash scripts/ci/helm-lint.sh`.
- 2026-05-18: gate de integración de RC pasado con
  `bash scripts/ci/integration-nats.sh` y
  `bash scripts/ci/integration-postgres.sh`; ambos usaron daemon Docker
  para testcontainers y cerraron verde.
- 2026-05-18: gate compose de RC pasado con `make e2e-compose`:
  build de imágenes, stack Docker Compose completo, escenarios 1-9,
  providers shape OpenAI/vLLM con `Report` válido y cierre con
  `E2E scenarios passed`.
- 2026-05-18: build/push de imagen validado con Podman y GHCR:
  `underpass-choreographer:sha-c74aac2` y
  `underpass-choreographer-e2e-runner:sha-c74aac2` publicados en
  `ghcr.io/underpass-ai` usando el token local de `/tmp/github.txt`.
- 2026-05-18: Kubernetes smoke de RC pasado contra namespace temporal
  `choreographer-e2e`: `make e2e-kubernetes` con imágenes GHCR
  `sha-c74aac2`, `ghcr-pull`, escenarios seleccionados 1-4,
  propagación causal por NATS y cierre con
  `E2E kubernetes scenarios passed`; namespace temporal eliminado.
- 2026-05-18: `make consumer-smoke` rejection-path validado contra
  RC temporal `sha-c74aac2` en namespace
  `choreographer-consumer-smoke`: la release antigua en
  `underpass-runtime` devolvía `RegisterContract=Unimplemented`, por
  eso se ejecutó contra el RC; `CONSUMER_SMOKE_CHAIN=two` pasó con
  `report_schema_registered` y
  `report_contract_rejects_freeform_text` en PASS.
- 2026-05-18: consumer-smoke positive-path validado contra el mismo
  RC temporal con `stub-llm` desplegado en Kubernetes y provider
  `openai` habilitado por env dummy; `CONSUMER_SMOKE_CHAIN=positive-path`
  pasó con 7/8 assertions, incluyendo `report_payload_validates` y
  `report_validation_summary_passed`; la assertion de bus quedó
  `Skipped` porque el CLI se ejecutó sin `CHOREO_NATS_URL`.
- 2026-05-18: la estrategia de publicación MCP queda reconciliada con
  `feat/bundle-b-mcp-distribution`: `choreo-mcp` depende del crate
  vendorizado `choreo-mcp-proto`, no del crate interno
  `choreo-proto`.
- 2026-05-18: `cargo publish --dry-run --allow-dirty -p choreo-mcp`
  queda pendiente porque `choreo-mcp-proto v0.1.0` aún no está en
  crates.io. El workflow de publicación debe publicar primero
  `choreo-mcp-proto`, esperar propagación del índice y después
  publicar `choreo-mcp`.
- 2026-05-18: dry-run de chart validado: `helm package` generó
  `/tmp/choreographer-chart-dry-run/choreographer-0.1.0.tgz`,
  `helm lint charts/choreographer` pasó y `helm registry login
  ghcr.io` funcionó con el token local. No se ejecutó `helm push`
  porque Helm no ofrece dry-run para OCI push y la publicación real
  debe esperar tag/release.
- 2026-05-18: release notes completadas con límites explícitos en
  `CHANGELOG.md`: ausencia de tag/artifact estable, dependencia de
  `choreo-mcp-proto` para publicar `choreo-mcp`, smokes provider con
  stub salvo wiring real del operador, e Ingress/egress/multi-réplica
  fuera del alcance actual del chart.
- 2026-05-18: `make e2e-provider-vllm` validado contra vLLM real en
  `https://vllm.underpassai.com` con modelo `google/gemma-4-31B-it`,
  imagen
  `ghcr.io/underpass-ai/underpass-choreographer-e2e-provider:sha-c74aac2`,
  secreto mTLS `underpass-demo-client-tls` y pull secret `ghcr-pull`;
  el Job terminó con `Job completed successfully`. Queda anotado como
  siguiente bloque el E2E de council multiagente con vLLM real.
- 2026-05-18: ceremonia E2E peer-review 360 añadida para vLLM real:
  registra varios agentes `kind=vllm`, crea council, ejecuta
  `RunCouncilDecision` en Strict y falla si no hay candidatos de
  autores distintos, winner schema-valid y `revision_count > 0`.
  También se expuso `revision_count` en gRPC y MCP JSON para que la
  interacción entre agentes sea observable. Validado con `cargo check`
  y `cargo test` sobre `choreo-e2e-runner`, `choreo-adapters` y
  `choreo-mcp`. Queda pendiente build/push de imagen y ejecución real
  del Job, más el E2E equivalente entrando por MCP.
- 2026-06-06: runner MCP live añadido para la misma ceremonia
  multiagente real vLLM: `make e2e-mcp-council-vllm` usa
  `choreo-mcp` por stdio para registrar contrato, agentes, council y
  ejecutar `RunCouncilDecision` en Strict. El runner valida múltiples
  candidatos, autores distintos, Report JSON válido y
  `revision_count > 0`. Validado sin endpoint real con
  `python3 scripts/mcp/choreo-mcp-council-vllm.py --self-test`,
  `python3 -m py_compile scripts/mcp/choreo-mcp-council-vllm.py`
  con `PYTHONPYCACHEPREFIX=/tmp/choreo-mcp-pycache`, y
  `bash -n scripts/ci/e2e-mcp-council-vllm.sh`; queda pendiente
  ejecución contra Choreographer + vLLM real.
