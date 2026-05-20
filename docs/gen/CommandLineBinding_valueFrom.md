If `valueFrom` is a constant string value, use this as the value and
apply the binding rules above.

If `valueFrom` is an expression, evaluate the expression to yield the
actual value to use to build the command line and apply the binding
rules above.  If the inputBinding is associated with an input
parameter, the value of `self` in the expression will be the value of
the input parameter.  Input parameter defaults (as specified by the
`InputParameter.default` field) must be applied before evaluating the
expression.

If the value of the associated input parameter is `null`, `valueFrom` is
not evaluated and nothing is added to the command line.

When a binding is part of the `CommandLineTool.arguments` field,
the `valueFrom` field is required.
