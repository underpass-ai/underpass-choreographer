# Plan Para Tener El Producto Usable Y Publicable

Fecha: 2026-05-18

Este plan asume que el producto a publicar es **Underpass
Choreographer como plano genérico de coordinación**, no PIR ni ningún
producto downstream concreto.

## Aclaración De Independencia

Choreographer es agnóstico e independiente. PIR, KMP,
`underpass-runtime` u otros repos aparecen en la documentación solo
como referencias de estudio, integraciones posibles o casos de uso.
No forman parte obligatoria del producto publicable.

La frontera estable del producto es:

- entrada de contexto: `ExternalContextBundle` suministrado por el
  caller, venga de KMP, RAG, una aplicación propia o cualquier otra
  fuente;
- ejecución: opcional, mediante adaptadores como `RuntimeExecutor`;
- dominio: siempre externo, expresado por configuración, atributos,
  contratos y payloads.

## Tesis

El repositorio ya tiene una base técnica sólida: gRPC completo, MCP
stdio, Runtime executor, contratos de salida con JSON Schema, NATS,
Postgres, TLS/mTLS, Helm y E2E con stubs.

Lo que falta para que sea usable y publicable no es más arquitectura.
Falta cerrar los caminos de uso reales, separar demos de producción y
convertir las pruebas actuales en una experiencia que un tercero pueda
ejecutar sin leer el código fuente.

## Promesa Pública

### Se Puede Prometer

- Coordina councils de agentes especializados.
- Expone la API gRPC completa `underpass.choreo.v1`.
- Expone MCP stdio para agentes como Codex y Claude Desktop.
- Acepta contexto externo mediante `ExternalContextBundle`.
- Puede ejecutar winners mediante `RuntimeExecutor`.
- Soporta contratos de salida con JSON Schema.
- Despliega mediante container + Helm.
- Publica y consume eventos por core NATS pub/sub.
- Tiene E2E reproducible con `stub-runtime` y `stub-llm`.

### No Se Debe Prometer Todavía

- Que Choreographer consulta KMP directamente o depende de KMP.
- Que NATS tiene semántica durable, replay o ack tipo JetStream.
- Que `StreamDeliberation` emite cada propuesta, crítica y revisión.
- Que un provider real está validado en CI sin credenciales externas.
- Que este repo implementa PIR o cualquier producto downstream.

## Fase 1 - Camino Local Usable

Objetivo: que alguien clone el repo y vea valor en menos de 15 minutos.

Trabajo:

- Crear `docs/operations/compose-e2e.md`.
- Documentar los 9 escenarios de `make e2e-compose`.
- Explicar `stub-runtime`, `stub-llm`, Report schema y los provider
  shapes OpenAI/vLLM.
- Añadir un quickstart local:
  - sin externos: `CHOREO_NATS_ENABLED=false just run`
  - demo completa: `make e2e-compose`
  - MCP fixture: `CHOREO_MCP_BACKEND=fixture choreo-mcp`
  - MCP live contra gRPC local.
- Añadir ejemplos concretos de requests para:
  - `CreateCouncil`
  - `RegisterAgent`
  - `RegisterContract`
  - `RunCouncilDecision`
  - `Orchestrate`

Criterio de salida:

- Un usuario nuevo puede ejecutar la demo, ver un council, registrar un
  contrato y obtener un resultado validado sin leer Rust.

## Fase 2 - Separar Demos De Producción

Objetivo: que los smoke tests no fallen por mezclar stubs con Runtime
real.

Trabajo:

- Hacer el e2e-runner seleccionable por escenarios:
  - `compose`: escenarios 1-9.
  - `cluster-connectivity`: escenarios 1-4.
  - `runtime-stub`: escenario 5.
  - `structured-output`: escenarios 6-9.
- Cambiar el Job Kubernetes para no ejecutar por defecto escenarios que
  requieren `stub.echo` o `stub-llm`.
- Mantener `make e2e-compose` como prueba completa repo-owned.
- Mantener `make e2e-provider-vllm` como prueba operator-run contra un
  provider real.

Criterio de salida:

- Kubernetes smoke pasa contra un deploy real.
- Compose E2E sigue probando todo el stack con fixtures.
- No hay "fallos esperados" en documentación operativa.

## Fase 3 - Consumer Smoke Integrable

Objetivo: que un downstream pueda poner Choreographer en su CI.

Trabajo:

- Extender `choreo-consumer-smoke` con modo positivo:
  - registrar agent `openai` o `vllm` contra un endpoint
    OpenAI-compatible.
  - registrar Report schema.
  - ejecutar `RunCouncilDecision` en Strict mode.
  - validar `report_payload_validates = Passed`.
- Mantener el modo actual NoopAgent como rejection-path.
- Documentar:
  - modo rejection-path
  - modo positive-path
  - NATS opcional
  - exit codes
  - ejemplo de CI para consumidores.

Criterio de salida:

- Un consumidor puede comprobar:
  - la API vive.
  - output inválido se rechaza.
  - output JSON válido se acepta.
  - correlation/causation se propaga por NATS cuando NATS está
    configurado.

## Fase 4 - Publicación Operativa

Objetivo: que un operador pueda instalarlo sin asistencia directa.

Trabajo:

- Completar guía Helm con:
  - instalación mínima.
  - instalación con embedded NATS.
  - instalación con Runtime executor.
  - instalación con TLS/mTLS.
  - instalación con Postgres secret.
  - instalación con provider env secrets.
- Añadir `SECURITY.md`.
- Añadir `CHANGELOG.md`.
- Añadir `CONTRIBUTING.md` mínimo.
- Añadir matriz de soporte:
  - Rust version.
  - image tags.
  - chart versions.
  - providers soportados.
  - postura Kubernetes soportada.
- Documentar rollback y upgrade con ejemplos reales.

Criterio de salida:

- Un operador puede desplegar imagen pinneada, chart OCI, secret refs y
  smoke post-deploy.

## Fase 5 - Release Candidate

Objetivo: generar una RC publicable, no solo mergear código.

Gates obligatorios:

- `just check`
- `just helm-lint`
- `just integration`
- `make e2e-compose`
- Kubernetes smoke con escenarios seleccionados.
- `make consumer-smoke` rejection-path.
- consumer-smoke positive-path contra `stub-llm` o provider compatible.
- `make e2e-provider-vllm` si se va a anunciar vLLM real.
- `cargo publish --dry-run` para `choreo-mcp-proto` y `choreo-mcp`.
- build/push dry run de imagen y chart.

Criterio de salida:

- Tag RC.
- Imagen GHCR.
- Helm chart OCI.
- `choreo-mcp` instalable.
- Release notes con límites explícitos.

## Fase 6 - Public Beta

Objetivo: publicarlo honestamente.

Publicar como:

- **Underpass Choreographer Public Beta**
- Generic agent council coordination plane.
- gRPC + MCP + Helm.
- Caller-supplied context.
- Runtime executor opcional.
- Structured outputs via JSON Schema.

No usar todavía:

- "production-ready" sin downstream smoke real.
- "KMP-integrated" si Choreographer no consulta KMP.
- "durable eventing".
- "real-time deliberation streaming" para turnos internos.

## Orden Recomendado

1. Crear `docs/operations/compose-e2e.md`.
2. Añadir selector de escenarios al e2e-runner.
3. Limpiar Kubernetes smoke para que pase contra un deploy real.
4. Añadir consumer-smoke positive-path.
5. Completar Helm/operator docs.
6. Añadir `SECURITY.md`, `CHANGELOG.md`, `CONTRIBUTING.md`.
7. Cortar release candidate.
8. Publicar public beta.

## No Hacer Ahora

- No meter PIR en este repo.
- No implementar JetStream hasta que un consumidor lo necesite.
- No añadir un adapter KMP propio si `ExternalContextBundle` basta.
- No priorizar streaming turn-level antes de cerrar instalación,
  smoke y release.

## Definición De Producto Usable

El producto es usable cuando alguien externo puede:

1. Instalarlo localmente o por Helm.
2. Crear o listar councils.
3. Registrar un agent.
4. Registrar un output contract.
5. Ejecutar `RunCouncilDecision`.
6. Recibir un resultado validado.
7. Conectarlo por MCP.
8. Ejecutar un smoke reproducible.

## Definición De Producto Publicable

El producto es publicable cuando además:

1. Los límites están documentados sin contradicciones.
2. El quickstart funciona desde cero.
3. Los smoke tests no tienen "fallos esperados".
4. La release produce imagen, chart y binario MCP versionados.
5. Hay guía de seguridad, contribución, changelog y rollback.
6. Las claims públicas están respaldadas por tests, E2E o docs de
   operación reproducibles.
