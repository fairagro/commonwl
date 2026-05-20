A workflow describes a set of **steps** and the **dependencies** between
those steps.  When a step produces output that will be consumed by a
second step, the first step is a dependency of the second step.

When there is a dependency, the workflow engine must execute the preceding
step and wait for it to successfully produce output before executing the
dependent step.  If two steps are defined in the workflow graph that
are not directly or indirectly dependent, these steps are **independent**,
and may execute in any order or execute concurrently.  A workflow is
complete when all steps have been executed.

Dependencies between parameters are expressed using the `source`
field on [workflow step input parameters](#WorkflowStepInput) and
`outputSource` field on [workflow output
parameters](#WorkflowOutputParameter).

The `source` field on each workflow step input parameter expresses
the data links that contribute to the value of the step input
parameter (the "sink").  A workflow step can only begin execution
when every data link connected to a step has been fulfilled.

The `outputSource` field on each workflow step input parameter
expresses the data links that contribute to the value of the
workflow output parameter (the "sink").  Workflow execution cannot
complete successfully until every data link connected to an output
parameter has been fulfilled.

## Workflow success and failure

A completed step must result in one of `success`, `temporaryFailure` or
`permanentFailure` states.  An implementation may choose to retry a step
execution which resulted in `temporaryFailure`.  An implementation may
choose to either continue running other steps of a workflow, or terminate
immediately upon `permanentFailure`.

* If any step of a workflow execution results in `permanentFailure`, then
the workflow status is `permanentFailure`.

* If one or more steps result in `temporaryFailure` and all other steps
complete `success` or are not executed, then the workflow status is
`temporaryFailure`.

* If all workflow steps are executed and complete with `success`, then the
workflow status is `success`.

# Extensions

[ScatterFeatureRequirement](#ScatterFeatureRequirement) and
[SubworkflowFeatureRequirement](#SubworkflowFeatureRequirement) are
available as standard [extensions](#Extensions_and_Metadata) to core
workflow semantics.
