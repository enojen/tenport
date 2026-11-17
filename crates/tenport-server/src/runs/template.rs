use std::collections::BTreeMap;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use k8s_openapi::api::batch::v1::Job;
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::{Map, Value};

use super::model::{ClassName, ServiceName};

const JOB_FILE: &str = "job.yaml";
const CLASSES_FILE: &str = "classes.yaml";
const SERVICE_EXTENSION: &str = "yaml";
const APP_LABEL: &str = "app";
const MANAGED_BY_LABEL: &str = "app.kubernetes.io/managed-by";
const MANAGER: &str = "tenport";
const LABEL_POINTERS: [&str; 2] = ["/metadata/labels", "/spec/template/metadata/labels"];
const POD_SPEC_POINTER: &str = "/spec/template/spec";
const INIT_CONTAINERS: &str = "initContainers";
const VOLUMES: &str = "volumes";
const NAME_KEY: &str = "name";
const BOX_CONTAINER: &str = "box";

#[derive(Debug, Clone)]
pub struct RunTemplate {
    pub dir: PathBuf,
    pub services_dir: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TemplateFields {
    pub name: String,
    pub task: String,
    pub run: String,
    pub project: String,
    pub project_id: String,
    pub repo: String,
    pub base: String,
    pub git_name: String,
    pub git_email: String,
    pub image: String,
    pub command: String,
    pub cmd: String,
    pub env: BTreeMap<String, String>,
    pub class: ClassName,
    pub archive_get: String,
    pub archive_put: String,
    pub setup: String,
}

#[derive(Debug, thiserror::Error)]
pub enum TemplateError {
    #[error("cannot read {}: {source}", path.display())]
    Read {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("{} is not what Tenport reads there: {detail}", path.display())]
    Invalid { path: PathBuf, detail: String },
    #[error("{} has no class {class}", path.display())]
    NoClass { path: PathBuf, class: ClassName },
    #[error("there is no service {service}")]
    NoService { service: ServiceName },
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Resources {
    cpu: String,
    memory: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ServicePatch {
    spec: ServiceJobSpec,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ServiceJobSpec {
    template: ServicePod,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ServicePod {
    spec: ServicePodSpec,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct ServicePodSpec {
    #[serde(default)]
    init_containers: Vec<Map<String, Value>>,
    #[serde(default)]
    containers: Vec<ServiceBox>,
    #[serde(default)]
    volumes: Vec<Map<String, Value>>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ServiceBox {
    name: String,
    env: Vec<EnvValue>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct EnvValue {
    name: String,
    value: String,
}

impl RunTemplate {
    pub fn job(
        &self,
        fields: &TemplateFields,
        services: &[ServiceName],
    ) -> Result<Job, TemplateError> {
        let resources = self.resources(&fields.class)?;
        let patches = services
            .iter()
            .map(|service| self.service(service))
            .collect::<Result<Vec<_>, _>>()?;
        let pairs = placeholders(fields, &resources, &box_env(fields, &patches));
        let path = self.dir.join(JOB_FILE);
        let mut job = fill(read_yaml::<Value>(&path)?, &pairs);
        add_services(&mut job, patches);
        mark_managed(&mut job);
        serde_json::from_value(job).map_err(|error| invalid(&path, error))
    }

    fn resources(&self, class: &ClassName) -> Result<Resources, TemplateError> {
        let path = self.dir.join(CLASSES_FILE);
        let mut classes: BTreeMap<String, Resources> = read_yaml(&path)?;
        classes
            .remove(class.as_str())
            .ok_or_else(|| TemplateError::NoClass {
                path,
                class: class.clone(),
            })
    }

    fn service(&self, service: &ServiceName) -> Result<ServicePodSpec, TemplateError> {
        let path = self
            .services_dir
            .join(service.as_str())
            .with_extension(SERVICE_EXTENSION);
        let spec = match read_yaml::<ServicePatch>(&path) {
            Err(TemplateError::Read { source, .. }) if source.kind() == ErrorKind::NotFound => {
                return Err(TemplateError::NoService {
                    service: service.clone(),
                });
            }
            other => other?.spec.template.spec,
        };
        if spec
            .containers
            .iter()
            .any(|container| container.name != BOX_CONTAINER)
        {
            return Err(invalid(
                &path,
                "a service adds env only to the container box",
            ));
        }
        Ok(spec)
    }
}

fn read_yaml<T: DeserializeOwned>(path: &Path) -> Result<T, TemplateError> {
    let text = std::fs::read_to_string(path).map_err(|source| TemplateError::Read {
        path: path.to_owned(),
        source,
    })?;
    serde_saphyr::from_str(&text).map_err(|error| invalid(path, error))
}

fn invalid(path: &Path, error: impl std::fmt::Display) -> TemplateError {
    TemplateError::Invalid {
        path: path.to_owned(),
        detail: error.to_string(),
    }
}

fn box_env(fields: &TemplateFields, patches: &[ServicePodSpec]) -> String {
    let services = patches
        .iter()
        .flat_map(|patch| &patch.containers)
        .flat_map(|container| &container.env)
        .map(|env| (env.name.clone(), env.value.clone()));
    let merged: BTreeMap<String, String> = services.chain(fields.env.clone()).collect();
    Value::from(Map::from_iter(
        merged.into_iter().map(|(k, v)| (k, Value::from(v))),
    ))
    .to_string()
}

fn placeholders(
    fields: &TemplateFields,
    resources: &Resources,
    env: &str,
) -> Vec<(&'static str, String)> {
    vec![
        ("__NAME__", fields.name.clone()),
        ("__TASK__", fields.task.clone()),
        ("__RUN__", fields.run.clone()),
        ("__PROJECT__", fields.project.clone()),
        ("__PROJECT_ID__", fields.project_id.clone()),
        ("__REPO__", fields.repo.clone()),
        ("__BASE__", fields.base.clone()),
        ("__GIT_NAME__", fields.git_name.clone()),
        ("__GIT_EMAIL__", fields.git_email.clone()),
        ("__IMAGE__", fields.image.clone()),
        ("__COMMAND__", fields.command.clone()),
        ("__CMD__", fields.cmd.clone()),
        ("__ENV__", env.to_owned()),
        ("__CLASS__", fields.class.as_str().to_owned()),
        ("__CPU__", resources.cpu.clone()),
        ("__MEMORY__", resources.memory.clone()),
        ("__ARCHIVE_GET__", fields.archive_get.clone()),
        ("__ARCHIVE_PUT__", fields.archive_put.clone()),
        ("__SETUP__", fields.setup.clone()),
    ]
}

fn fill(value: Value, pairs: &[(&str, String)]) -> Value {
    match value {
        Value::String(text) => Value::String(fill_text(text, pairs)),
        Value::Array(items) => {
            Value::Array(items.into_iter().map(|item| fill(item, pairs)).collect())
        }
        Value::Object(map) => {
            Value::Object(map.into_iter().map(|(k, v)| (k, fill(v, pairs))).collect())
        }
        other => other,
    }
}

fn fill_text(text: String, pairs: &[(&str, String)]) -> String {
    pairs
        .iter()
        .fold(text, |out, (field, value)| out.replace(field, value))
}

fn add_services(job: &mut Value, patches: Vec<ServicePodSpec>) {
    let Some(pod) = job
        .pointer_mut(POD_SPEC_POINTER)
        .and_then(Value::as_object_mut)
    else {
        return;
    };
    for patch in patches {
        replace_by_name(pod, INIT_CONTAINERS, patch.init_containers);
        replace_by_name(pod, VOLUMES, patch.volumes);
    }
}

fn replace_by_name(pod: &mut Map<String, Value>, key: &str, added: Vec<Map<String, Value>>) {
    let names: Vec<Value> = added
        .iter()
        .filter_map(|entry| entry.get(NAME_KEY).cloned())
        .collect();
    let list = pod.entry(key).or_insert_with(|| Value::Array(Vec::new()));
    if let Value::Array(items) = list {
        items.retain(|item| item.get(NAME_KEY).is_none_or(|name| !names.contains(name)));
        items.extend(added.into_iter().map(Value::Object));
    }
}

fn mark_managed(job: &mut Value) {
    for pointer in LABEL_POINTERS {
        if let Some(labels) = job.pointer_mut(pointer).and_then(Value::as_object_mut) {
            labels.remove(APP_LABEL);
            labels.insert(MANAGED_BY_LABEL.to_owned(), Value::from(MANAGER));
        }
    }
}

#[cfg(test)]
mod tests;
