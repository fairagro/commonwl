To use valueFrom, [`StepInputExpressionRequirement`](#StepInputExpressionRequirement) must
be specified in the workflow or workflow step requirements.

If `valueFrom` is a constant string value, use this as the value for
this input parameter.

If `valueFrom` is a parameter reference or expression, it must be
evaluated to yield the actual value to be assigned to the input field.

The `self` value in the parameter reference or expression must be

1. `null` if there is no `source` field
1. the value of the parameter(s) specified in the `source` field when this
   workflow input parameter **is not** specified in this workflow step's `scatter` field.
1. an element of the parameter specified in the `source` field when this workflow input
   parameter **is** specified in this workflow step's `scatter` field.

The value of `inputs` in the parameter reference or expression must be
the input object to the workflow step after assigning the `source`
values, applying `default`, and then scattering. The order of
evaluating `valueFrom` among step input parameters is undefined and the
result of evaluating `valueFrom` on a parameter must not be visible to
evaluation of `valueFrom` on other parameters.
