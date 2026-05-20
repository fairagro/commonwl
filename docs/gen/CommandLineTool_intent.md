An identifier for the type of computational operation, of this Process.
Especially useful for [`Operation`](Workflow.html#Operation), but can also be used for
[`CommandLineTool`](CommandLineTool.html#CommandLineTool),
[`Workflow`](Workflow.html#Workflow), or [ExpressionTool](Workflow.html#ExpressionTool).

If provided, then this must be an IRI of a concept node that
represents the type of operation, preferably defined within an ontology.

For example, in the domain of bioinformatics, one can use an IRI from
the EDAM Ontology's [Operation concept nodes](http://edamontology.org/operation_0004),
like [Alignment](http://edamontology.org/operation_2928),
or [Clustering](http://edamontology.org/operation_3432); or a more
specific Operation concept like
[Split read mapping](http://edamontology.org/operation_3199).
