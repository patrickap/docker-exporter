use bollard::{container::Stats, secret::ContainerState, Docker};
use futures::future;
use std::sync::Arc;
use tokio::task::JoinError;

use crate::docker::DockerExt;

pub struct Container {
  pub id: Option<String>,
  pub name: Option<String>,
  pub state: Option<ContainerState>,
  pub stats: Option<Stats>,
}

pub struct Containers {}

impl Containers {
  pub fn new() -> Self {
    Self {}
  }

  pub async fn retrieve(docker: Arc<Docker>) -> Result<Vec<Container>, JoinError> {
    let containers = docker.list_containers_all().await.unwrap_or_default();

    let result = containers.into_iter().map(|container| {
      let docker = Arc::clone(&docker);

      tokio::spawn(async move {
        let container = Arc::new(container);

        let state = {
          let docker = Arc::clone(&docker);
          let container = Arc::clone(&container);
          tokio::spawn(async move { docker.inspect_state(&container.id.as_deref()?).await })
        };

        let stats = {
          let docker = Arc::clone(&docker);
          let container = Arc::clone(&container);
          tokio::spawn(async move { docker.stats_once(&container.id.as_deref()?).await })
        };

        let (state, stats) = tokio::join!(state, stats);
        let (state, stats) = (state.ok().flatten(), stats.ok().flatten());

        Container {
          id: stats.as_ref().map(|s| String::from(&s.id)),
          name: stats.as_ref().map(|s| String::from(&s.name[1..])),
          state,
          stats,
        }
      })
    });

    future::try_join_all(result).await
  }
}
