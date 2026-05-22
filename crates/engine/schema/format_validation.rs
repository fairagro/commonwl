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
    fs::{self, File},
    io::BufReader,
    path::Path,
    sync::Arc,
    vec,
};

#[derive(Debug)]
pub(crate) struct FormatValidator {
    pub namespaces: HashMap<String, String>,
    pub ontologies: Vec<Ontology>,
}

#[derive(Debug)]
pub(crate) enum Ontology {
    Graph(Graph),
    SetOntology(SetOntology<ArcStr>),
}

const SUBCLASS_OF: &str = "http://www.w3.org/2000/01/rdf-schema#subclassOf";
const EQUIVALENT_CLASS: &str = "http://www.w3.org/2002/07/owl#equivalentClass";

impl FormatValidator {
    pub(crate) fn new(
        namespaces: &HashMap<String, String>,
        ontologies: Vec<impl AsRef<Path> + std::fmt::Debug>,
    ) -> anyhow::Result<Self> {
        let mut ontos = vec![];
        for path in ontologies {
            if path.as_ref().extension().is_some_and(|ext| ext == "ttl") {
                let content = fs::read_to_string(&path)
                    .with_context(|| format!("Could not read ttl {path:?}"))?;
                let mut reader = TurtleParser::from_string(content);
                let graph = reader.decode().map_err(|e| anyhow::anyhow!("{e}"))?;
                ontos.push(Ontology::Graph(graph));
            }
            if path.as_ref().extension().is_some_and(|ext| ext == "owl") {
                let file = File::open(&path).with_context(|| format!("Could not open {path:?}"))?;
                let mut bufreader = BufReader::new(file);
                let b = Build::new_arc();
                let onot: ParserOutput<ArcStr, Arc<AnnotatedComponent<ArcStr>>> = ParserOutput::rdf(
                    horned_owl::io::rdf::reader::read_with_build(
                        &mut bufreader,
                        &b,
                        ParserConfiguration::default(),
                    )
                    .unwrap(),
                );
                let (a, _, _) = onot.decompose();
                ontos.push(Ontology::SetOntology(a));
            }
        }
        Ok(Self {
            namespaces: namespaces.clone(),
            ontologies: ontos,
        })
    }

    pub(crate) fn validate(&self, format_a: &str, format_b: &str) -> bool {
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
                    let b = Build::new_arc();

                    //check equivalent class
                    let ec =
                        EquivalentClasses(vec![b.class(format_a).into(), b.class(format_b).into()]);
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
                *t_format = value.as_str().unwrap().to_string();
            }

            if let Some((namespace, value)) = t_format.split_once(':')
                && let Some(resolved) = self.namespaces.get(namespace)
            {
                format = Some(format!("{resolved}{value}"));
            }
        }
        format.clone()
    }
}

pub(crate) fn get_format_validator(
    doc: &CWLDocument,
    working_dir: &Path,
) -> anyhow::Result<FormatValidator> {
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
                .filter_map(|v| Some(working_dir.join(v.as_str()?)))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    FormatValidator::new(&namespaces, schemas)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    #[cfg(not(target_os = "windows"))] //cwl submodule is not available on windows
    fn test_validate_format() {
        let namespaces = HashMap::new();
        let fv = FormatValidator::new(
            &namespaces,
            [
                Path::new("../../testdata/cwl/tests/EDAM.owl"),
                Path::new("../../testdata/cwl/tests/gx_edam.ttl"),
            ]
            .into(),
        )
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
}
