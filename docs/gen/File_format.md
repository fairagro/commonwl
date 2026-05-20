The format of the file: this must be an IRI of a concept node that
represents the file format, preferably defined within an ontology.
If no ontology is available, file formats may be tested by exact match.

Reasoning about format compatibility must be done by checking that an
input file format is the same, `owl:equivalentClass` or
`rdfs:subClassOf` the format required by the input parameter.
`owl:equivalentClass` is transitive with `rdfs:subClassOf`, e.g. if
`<B> owl:equivalentClass <C>` and `<B> owl:subclassOf <A>` then infer
`<C> owl:subclassOf <A>`.

`File` format ontologies may be provided in the "$schemas" metadata at the
root of the document. If no ontologies are specified in `$schemas`, the
runtime may perform exact file format matches.
