The sorting key.  Default position is 0. If a [CWL Parameter Reference](#Parameter_references)
or [CWL Expression](#Expressions_(Optional)) is used and if the
inputBinding is associated with an input parameter, then the value of
`self` will be the value of the input parameter.  Input parameter
defaults (as specified by the `InputParameter.default` field) must be
applied before evaluating the expression. Expressions must return a
single value of type int or a null.
