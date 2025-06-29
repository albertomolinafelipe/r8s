use shared::models::PodObject;
use std::collections::HashMap;
use std::sync::Arc;
use bollard::{
    container::{
        Config, 
        CreateContainerOptions, 
        InspectContainerOptions, 
        StartContainerOptions}, 
    image::CreateImageOptions, 
    Docker
};
use futures_util::stream::TryStreamExt;
use crate::state::{ContainerRuntime, PodRuntime, State};


pub fn docker_client() -> Arc<Docker> {
    Arc::new(Docker::connect_with_local_defaults().expect("Failed to connect to Docker daemon"))
}


pub async fn start_pod(docker: Arc<Docker>, state: &State, pod: PodObject) -> PodRuntime {

    let mut container_runtimes = Vec::new();
    for container_spec in &pod.spec.containers {
        ensure_image(&docker, state, &container_spec.image).await;

        let container_name = format!("pod_{}_{}", pod.metadata.user.name, container_spec.name);

        let config = Config {
            image: Some(container_spec.image.clone()),
            env: Some(
                container_spec
                    .env
                    .iter()
                    .map(|env| format!("{}={}", env.name, env.value))
                    .collect(),
            ),
            exposed_ports: Some(
                container_spec
                    .ports
                    .iter()
                    .map(|p| (format!("{}/tcp", p.container_port), HashMap::new()))
                    .collect(),
            ),
            ..Default::default()
        };

        let options = Some(CreateContainerOptions {
            name: &container_name, platform: None
        });

        let create_response = docker
            .create_container(options, config)
            .await
            .expect("Failed to create container");

        let container_id = create_response.id;

        docker
            .start_container(&container_id, None::<StartContainerOptions<String>>)
            .await
            .expect("Failed to start container");

        let inspection = docker
            .inspect_container(&container_id, None::<InspectContainerOptions>)
            .await
            .expect("Failed to inspect container");

        let status = inspection
            .state
            .as_ref()
            .and_then(|s| s.status.clone())
            .unwrap();

        tracing::info!("Started container: id={} status={}", container_id, status);
        container_runtimes.push(ContainerRuntime {
            id: container_id,
            status,
        });
    }

    PodRuntime {
        id: pod.id,
        containers: container_runtimes,
    }
}


async fn ensure_image(docker: &Docker, state: &State, image: &str) {
    if state.has_image(image) {
        return;
    }

    let options = Some(CreateImageOptions {
        from_image: image,
        ..Default::default()
    });

    let mut stream = docker.create_image(options, None, None);
    while let Some(_status) = stream.try_next().await.unwrap_or(None) {
        // logging?
    }
    tracing::info!("Pulled container image={}", image);
    state.mark_image_as_pulled(image.to_string());
}
