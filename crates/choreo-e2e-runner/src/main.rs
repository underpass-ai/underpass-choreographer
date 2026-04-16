//! End-to-end runner.
//!
//! Connects to a running Choreographer over gRPC and executes a
//! sequence of scenarios that only pass if the entire stack
//! (bin + in-memory adapters + gRPC surface + seeding) is wired
//! correctly. Intended to run inside the docker-compose stack
//! defined under `tests/e2e/`.
//!
//! Exits 0 on success, non-zero on the first failed assertion.

use std::time::Duration;

use anyhow::{anyhow, bail, Context, Result};
use choreo_proto::v1::choreographer_service_client::ChoreographerServiceClient;
use choreo_proto::v1::{DeleteCouncilRequest, DeliberateRequest, ListCouncilsRequest, Task};
use tonic::transport::{Channel, Endpoint};
use tracing::{info, warn};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .compact()
        .init();

    let endpoint = std::env::var("CHOREOGRAPHER_ENDPOINT")
        .unwrap_or_else(|_| "http://choreographer:50055".to_owned());
    let seed_specialty =
        std::env::var("CHOREO_SEED_SPECIALTY").unwrap_or_else(|_| "triage".to_owned());

    let mut client = connect_with_retry(&endpoint, Duration::from_secs(30)).await?;

    info!("scenario 1: seeded council is visible");
    let councils = client
        .list_councils(ListCouncilsRequest {
            include_agents: false,
        })
        .await
        .context("ListCouncils failed")?
        .into_inner()
        .councils;
    if councils.is_empty() {
        bail!(
            "expected at least one seeded council — did the choreographer start with CHOREO_SEED_SPECIALTIES?"
        );
    }
    if !councils.iter().any(|c| c.specialty == seed_specialty) {
        bail!(
            "seeded specialty {seed_specialty} not found among {:?}",
            councils.iter().map(|c| &c.specialty).collect::<Vec<_>>()
        );
    }

    info!("scenario 2: Deliberate on the seeded specialty returns a winner");
    let response = client
        .deliberate(DeliberateRequest {
            task: Some(Task {
                task_id: "e2e-task-1".to_owned(),
                specialty: seed_specialty.clone(),
                description: "End-to-end test: describe the situation.".to_owned(),
                constraints: None,
                attributes: None,
            }),
        })
        .await
        .context("Deliberate failed")?
        .into_inner();

    if response.task_id != "e2e-task-1" {
        bail!("response.task_id = {:?}", response.task_id);
    }
    if response.winner_proposal_id.is_empty() {
        bail!("winner_proposal_id is empty");
    }
    if response.results.is_empty() {
        bail!("results[] is empty");
    }
    let winner = response
        .results
        .iter()
        .find(|r| r.rank == 0)
        .ok_or_else(|| anyhow!("no result with rank=0"))?;
    let winner_id = winner
        .proposal
        .as_ref()
        .map(|p| p.proposal_id.clone())
        .ok_or_else(|| anyhow!("rank=0 result has no proposal"))?;
    if winner_id != response.winner_proposal_id {
        bail!(
            "rank=0 proposal id {} disagrees with winner_proposal_id {}",
            winner_id,
            response.winner_proposal_id
        );
    }

    info!("scenario 3: DeleteCouncil on a missing specialty returns deleted=false");
    let delete = client
        .delete_council(DeleteCouncilRequest {
            specialty: "unknown-specialty".to_owned(),
        })
        .await
        .context("DeleteCouncil(unknown) failed")?
        .into_inner();
    if delete.deleted {
        bail!("DeleteCouncil on an unknown specialty must return deleted=false");
    }

    info!("E2E compose scenarios passed");
    Ok(())
}

async fn connect_with_retry(
    endpoint: &str,
    total_budget: Duration,
) -> Result<ChoreographerServiceClient<Channel>> {
    let deadline = std::time::Instant::now() + total_budget;
    let endpoint_parsed: Endpoint = endpoint
        .parse()
        .with_context(|| format!("invalid gRPC endpoint: {endpoint}"))?;

    let mut last_err: Option<tonic::transport::Error> = None;
    while std::time::Instant::now() < deadline {
        match ChoreographerServiceClient::connect(endpoint_parsed.clone()).await {
            Ok(c) => {
                info!(endpoint, "connected");
                return Ok(c);
            }
            Err(err) => {
                warn!(endpoint, error = %err, "not ready yet; will retry");
                last_err = Some(err);
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        }
    }
    Err(anyhow!(
        "could not connect to {endpoint} within {total_budget:?}: {last_err:?}"
    ))
}
