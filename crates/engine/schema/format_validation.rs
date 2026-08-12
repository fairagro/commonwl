use crate::expression::{EvaluationContext, do_eval};
use anyhow::Context;
use cwl_core::documents::CWLDocument;
use horned_owl::{
    io::{ParserConfiguration, ParserOutput},
    model::{
        AnnotatedComponent, ArcStr, Build, ClassExpression, Component, EquivalentClasses,
        SubClassOf,
    },
    ontology::set::SetOntology,
};
use rdf::{
    graph::Graph,
    node::Node,
    reader::{rdf_parser::RdfParser, turtle_parser::TurtleParser},
    uri::Uri,
};
use std::{
    collections::{BTreeSet, HashMap, HashSet, VecDeque},
    env, fs,
    io::{BufReader, Cursor},
    path::{Path, PathBuf},
    sync::{Arc, LazyLock},
    vec,
};
use tracing::debug;
use url::Url;

#[derive(Debug)]
pub struct FormatValidator {
    pub namespaces: HashMap<String, String>,
    pub ontologies: Vec<Ontology>,
}

#[derive(Debug)]
pub enum Ontology {
    Graph(Graph),
    SetOntology(SetOntology<ArcStr>),
}

enum SchemaKind {
    Ttl,
    Owl,
}

const SUBCLASS_OF: &str = "http://www.w3.org/2000/01/rdf-schema#subclassOf";
const EQUIVALENT_CLASS: &str = "http://www.w3.org/2002/07/owl#equivalentClass";
static EDAM_CACHE_PATH: LazyLock<PathBuf> = LazyLock::new(|| {
    env::temp_dir()
        .join(env!("CARGO_CRATE_NAME"))
        .join("EDAM.owl")
});
const EDAM_REMOTE_PATH: &str = "https://edamontology.org/EDAM.owl";

impl FormatValidator {
    pub(crate) async fn new(
        namespaces: &HashMap<String, String>,
        schemas: &[String],
        working_dir: &Path,
    ) -> crate::Result<Self> {
        let mut ontos = vec![];
        for entry in schemas {
            let Some((kind, bytes)) = Self::load_schema(entry, working_dir).await? else {
                continue;
            };
            match kind {
                SchemaKind::Ttl => {
                    let content = String::from_utf8(bytes)
                        .with_context(|| format!("Schema {entry} is not valid UTF-8"))?;
                    let mut reader = TurtleParser::from_string(content);
                    match reader.decode() {
                        Ok(graph) => ontos.push(Ontology::Graph(graph)),
                        Err(e) => Self::handle_parse_error(entry, &e.to_string())?,
                    }
                }
                SchemaKind::Owl => {
                    let mut bufreader = BufReader::new(Cursor::new(bytes));
                    let b = Build::new_arc();
                    match horned_owl::io::rdf::reader::read_with_build(
                        &mut bufreader,
                        &b,
                        ParserConfiguration::default(),
                    ) {
                        Ok(parsed) => {
                            let output: ParserOutput<ArcStr, Arc<AnnotatedComponent<ArcStr>>> =
                                ParserOutput::rdf(parsed);
                            let (a, _, _) = output.decompose();
                            ontos.push(Ontology::SetOntology(a));
                        }
                        Err(e) => Self::handle_parse_error(entry, &e.to_string())?,
                    }
                }
            }
        }
        Ok(Self {
            namespaces: namespaces.clone(),
            ontologies: ontos,
        })
    }

    //remote schemas are best-effort
    fn handle_parse_error(entry: &str, error: &str) -> crate::Result<()> {
        if Self::is_remote(entry) {
            tracing::warn!("Could not parse schema {entry}: {error}");
            Ok(())
        } else {
            Err(anyhow::anyhow!("Could not parse schema {entry}: {error}").into())
        }
    }

    fn is_remote(entry: &str) -> bool {
        Url::parse(entry).is_ok_and(|url| matches!(url.scheme(), "http" | "https"))
    }

    async fn load_schema(
        entry: &str,
        working_dir: &Path,
    ) -> crate::Result<Option<(SchemaKind, Vec<u8>)>> {
        let entry = if entry == EDAM_REMOTE_PATH && fs::exists(&*EDAM_CACHE_PATH)? {
            debug!(
                "Resolving EDAM ontology from cache at {}",
                EDAM_CACHE_PATH.display()
            );
            EDAM_REMOTE_PATH
        } else {
            entry
        };
        let bytes = if Self::is_remote(entry) {
            match reqwest::get(entry)
                .await
                .and_then(reqwest::Response::error_for_status)
            {
                Ok(response) => match response.bytes().await {
                    Ok(bytes) => {
                        let bytes = bytes.to_vec();
                        //write tempfile
                        if entry == EDAM_REMOTE_PATH {
                            debug!(
                                "Writing EDAM ontology to cache at {}",
                                EDAM_CACHE_PATH.display()
                            );
                            if let Some(parent) = EDAM_CACHE_PATH.parent() {
                                tokio::fs::create_dir_all(parent).await?;
                            }
                            tokio::fs::write(&*EDAM_CACHE_PATH, &bytes).await?;
                        }
                        bytes
                    }
                    Err(e) => {
                        tracing::warn!("Could not download schema {entry}: {e}");
                        return Ok(None);
                    }
                },
                Err(e) => {
                    tracing::warn!("Could not download schema {entry}: {e}");
                    return Ok(None);
                }
            }
        } else {
            let path = working_dir.join(entry);
            fs::read(&path).with_context(|| format!("Could not read schema {}", path.display()))?
        };

        let extension = Path::new(entry).extension();
        let kind = if extension.is_some_and(|ext| ext == "ttl") {
            Some(SchemaKind::Ttl)
        } else if extension.is_some_and(|ext| ext == "owl") {
            Some(SchemaKind::Owl)
        } else {
            None
        };

        Ok(kind.map(|kind| (kind, bytes)))
    }

    pub(crate) fn validate(&self, format_a: &str, format_b: &str) -> bool {
        let format_a = self.resolve_namespace(format_a);
        let format_a = format_a.as_str();
        let format_b = self.resolve_namespace(format_b);
        let format_b = format_b.as_str();

        //same formats, we are good to go and do not need any work
        if format_a == format_b {
            return true;
        }

        for ontology in &self.ontologies {
            match ontology {
                Ontology::Graph(graph) => {
                    let node_a = graph.create_uri_node(&Uri::new(format_a.to_string()));
                    let node_b = graph.create_uri_node(&Uri::new(format_b.to_string()));

                    //check equivalent class
                    let equivalent = graph.create_uri_node(&Uri::new(EQUIVALENT_CLASS.to_string()));
                    let eq_triples_a =
                        graph.get_triples_with_subject_and_predicate(&node_a, &equivalent);
                    let eq_triples_b =
                        graph.get_triples_with_subject_and_predicate(&node_b, &equivalent);

                    if eq_triples_a.iter().any(|i| i.object() == &node_b)
                        || eq_triples_b.iter().any(|i| i.object() == &node_a)
                    {
                        return true;
                    }

                    //check subclass of
                    let subclass = graph.create_uri_node(&Uri::new(SUBCLASS_OF.to_string()));
                    let mut visited = HashSet::new();
                    let mut queue = VecDeque::new();

                    queue.push_back(node_a.clone());
                    visited.insert(format_a.to_string());
                    while let Some(current) = queue.pop_front() {
                        let sub_triples =
                            graph.get_triples_with_subject_and_predicate(&current, &subclass);

                        for triple in sub_triples {
                            //found triple
                            if triple.object() == &node_b {
                                return true;
                            }

                            if let Node::UriNode { uri: node } = triple.object() {
                                let uri_str = node.to_string();
                                if !visited.contains(uri_str) {
                                    visited.insert(uri_str.clone());
                                    queue.push_back(triple.object().clone());
                                }
                            }
                        }
                    }
                }
                Ontology::SetOntology(set_ontology) => {
                    if Self::validate_set_ontology(set_ontology, format_a, format_b) {
                        return true;
                    }
                }
            }
        }

        false
    }

    //expands a CURIE-style `prefix:value` format into its full IRI via `$namespaces`, if the
    //prefix is known; otherwise returns the format unchanged
    fn resolve_namespace(&self, format: &str) -> String {
        if let Some((namespace, value)) = format.split_once(':')
            && let Some(resolved) = self.namespaces.get(namespace)
        {
            return format!("{resolved}{value}");
        }
        format.to_string()
    }

    fn validate_set_ontology(
        set_ontology: &SetOntology<ArcStr>,
        format_a: &str,
        format_b: &str,
    ) -> bool {
        let b = Build::new_arc();

        //check equivalent class
        let ec = EquivalentClasses(vec![b.class(format_a).into(), b.class(format_b).into()]);
        if set_ontology.i().contains(&AnnotatedComponent::new(
            Component::EquivalentClasses(ec),
            BTreeSet::default(),
        )) {
            return true;
        }

        //check subclass of
        let mut visited = HashSet::new();
        let mut queue: VecDeque<String> = VecDeque::new();

        queue.push_back(format_a.to_string());
        visited.insert(format_a.to_string());

        while let Some(current) = queue.pop_front() {
            let sc = SubClassOf {
                sub: b.class(&*current).into(),
                sup: b.class(format_b).into(),
            };

            if set_ontology.i().contains(&AnnotatedComponent::new(
                Component::SubClassOf(sc),
                BTreeSet::default(),
            )) {
                return true;
            }

            for axiom in set_ontology.i().iter() {
                if let Component::SubClassOf(SubClassOf { sub, sup }) = &axiom.component
                    && let ClassExpression::Class(sub_class) = sub
                {
                    let sup_str = sub_class.to_string();
                    if sup_str == current
                        && let ClassExpression::Class(sup_class) = sup
                    {
                        let sup_str = sup_class.0.to_string();
                        if !visited.contains(&sup_str) {
                            visited.insert(sup_str.clone());
                            queue.push_back(sup_str);
                        }
                    }
                }
            }
        }

        false
    }

    pub(crate) fn handle(
        &self,
        format: Option<&String>,
        context: Option<&EvaluationContext>,
    ) -> Option<String> {
        let mut format = format.cloned();
        //format accepts expression
        if let Some(t_format) = &mut format {
            if let Some(context) = context
                && let Ok(value) = do_eval(t_format, context)
            {
                //only string expressions are valid here
                *t_format = value.as_str().unwrap().to_string();
            }

            *t_format = self.resolve_namespace(t_format);
        }
        format.clone()
    }
}

pub(crate) async fn get_format_validator(
    doc: &CWLDocument,
    working_dir: &Path,
) -> crate::Result<FormatValidator> {
    let extension_fields = match doc {
        CWLDocument::CommandLineTool(clt) => &clt.extension_fields,
        CWLDocument::ExpressionTool(et) => &et.extension_fields,
        CWLDocument::Operation(op) => &op.extension_fields,
        CWLDocument::Workflow(wf) => &wf.extension_fields,
    };

    let namespaces = extension_fields
        .get("$namespaces")
        .and_then(|v| v.as_object())
        .map(|mapping| {
            mapping
                .iter()
                .filter_map(|(k, v)| {
                    let key = k.clone();
                    let value = v.as_str()?.to_string();
                    Some((key, value))
                })
                .collect::<HashMap<_, _>>()
        })
        .unwrap_or_default();

    let schemas = extension_fields
        .get("$schemas")
        .and_then(|v| v.as_array())
        .map(|vec| {
            vec.iter()
                .filter_map(|v| Some(v.as_str()?.to_string()))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    FormatValidator::new(&namespaces, &schemas, working_dir).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[cfg(not(target_os = "windows"))] //cwl submodule is not available on windows
    async fn test_validate_format() {
        let namespaces = HashMap::new();
        let fv = FormatValidator::new(
            &namespaces,
            &[
                "../../testdata/cwl/tests/EDAM.owl".to_string(),
                "../../testdata/cwl/tests/gx_edam.ttl".to_string(),
            ],
            Path::new("."),
        )
        .await
        .unwrap();
        //those are equivalent
        assert!(fv.validate(
            "http://galaxyproject.org/formats/fasta",
            "http://edamontology.org/format_1929"
        ));

        //transitive subclasses
        assert!(fv.validate(
            "http://edamontology.org/format_1929",
            "http://edamontology.org/format_2330",
        ));
    }

    #[tokio::test]
    #[cfg(not(target_os = "windows"))] //cwl submodule is not available on windows
    async fn test_validate_format_with_schemas() {
        let fv = FormatValidator::new(
            &HashMap::from([("edam".to_string(), "http://edamontology.org/".to_string())]),
            &["../../testdata/cwl/tests/EDAM.owl".to_string()],
            Path::new("."),
        )
        .await
        .unwrap();

        //textual stays true
        assert!(fv.validate("abc", "abc"));

        assert!(fv.validate(
            "http://edamontology.org/format_1929",
            "http://edamontology.org/format_2330",
        ));

        // prefix works if in namespace
        assert!(fv.validate("edam:format_1929", "edam:format_2330"));
    }

    /// without an explicit `$schemas` entry there is no ontology to reason with - only exact
    /// (post-namespace-resolution) string matches succeed, same as cwltool.
    #[tokio::test]
    async fn test_validate_format_fails_without_schema() {
        let fv = FormatValidator::new(
            &HashMap::from([("edam".to_string(), "http://edamontology.org/".to_string())]),
            &[],
            Path::new("."),
        )
        .await
        .unwrap();

        //exact match still fine, no ontology needed
        assert!(fv.validate("edam:format_1929", "edam:format_1929"));

        //format_1929 (FASTA) is a subclass of format_2330 (Textual format) in EDAM, but with no
        //$schemas loaded we must not know that
        assert!(!fv.validate("edam:format_1929", "edam:format_2330"));
    }

    #[tokio::test]
    async fn test_validate_format_warns_and_skips_unreachable_remote_schema() {
        //a broken/unreachable URL must not fail the whole validator - just skip that schema
        let fv = FormatValidator::new(
            &HashMap::new(),
            &["http://127.0.0.1:0/does-not-exist.owl".to_string()],
            Path::new("."),
        )
        .await
        .unwrap();

        assert!(fv.ontologies.is_empty());
    }

    #[ignore = "hits the real network"]
    #[tokio::test]
    async fn test_validate_format_fetches_remote_schema() {
        let fv = FormatValidator::new(
            &HashMap::new(),
            &["https://edamontology.org/EDAM.owl".to_string()],
            Path::new("."),
        )
        .await
        .unwrap();

        assert!(fv.validate(
            "http://edamontology.org/format_1929",
            "http://edamontology.org/format_2330",
        ));
    }
}
