A workflow step is an executable element of a workflow. It specifies the
underlying process implementation (such as `CommandLineTool` or another
`Workflow`) in the `run` field and connects the input and output parameters
of the underlying process to workflow parameters.

# Scatter/gather

To use scatter/gather,
[`ScatterFeatureRequirement`](#ScatterFeatureRequirement) must be specified
in the workflow or workflow step requirements.

A "scatter" operation specifies that the associated workflow step or
subworkflow should execute separately over a list of input elements. Each
job making up a scatter operation is independent and may be executed
concurrently.

The `scatter` field specifies one or more input parameters which will be
scattered. An input parameter may be listed more than once. The declared
type of each input parameter implicitly becomes an array of items of the
input parameter type. If a parameter is listed more than once, it becomes
a nested array. As a result, upstream parameters which are connected to
scattered parameters must be arrays.

All output parameter types are also implicitly wrapped in arrays. Each job
in the scatter results in an entry in the output array.

If any scattered parameter runtime value is an empty array, all outputs are
set to empty arrays and no work is done for the step, according to
applicable scattering rules.

If `scatter` declares more than one input parameter, `scatterMethod`
describes how to decompose the input into a discrete set of jobs.

- **dotproduct** specifies that each of the input arrays are aligned and one
  element taken from each array to construct each job. It is an error
  if all input arrays are not the same length.

- **`nested_crossproduct`** specifies the Cartesian product of the inputs,
  producing a job for every combination of the scattered inputs. The
  output must be nested arrays for each level of scattering, in the
  order that the input arrays are listed in the `scatter` field.

- **`flat_crossproduct`** specifies the Cartesian product of the inputs,
  producing a job for every combination of the scattered inputs. The
  output arrays must be flattened to a single level, but otherwise listed in the
  order that the input arrays are listed in the `scatter` field.

# Conditional execution (Optional)

Conditional execution makes execution of a step conditional on an
expression. A step that is not executed is "skipped". A skipped
step produces `null` for all output parameters.

The condition is evaluated after `scatter`, using the input object
of each individual scatter job. This means over a set of scatter
jobs, some may be executed and some may be skipped. When the
results are gathered, skipped steps must be `null` in the output
arrays.

The `when` field controls conditional execution. This is an
expression that must be evaluated with `inputs` bound to the step
input object (or individual scatter job), and returns a boolean
value. It is an error if this expression returns a value other
than `true` or `false`.

Conditionals in CWL are an optional feature and are not required
to be implemented by all consumers of CWL documents. An
implementation that does not support conditionals must return a
fatal error when attempting to execute a workflow that uses
conditional constructs the implementation does not support.

# Subworkflows

To specify a nested workflow as part of a workflow step,
[`SubworkflowFeatureRequirement`](#SubworkflowFeatureRequirement) must be
specified in the workflow or workflow step requirements.

It is a fatal error if a workflow directly or indirectly invokes itself as
a subworkflow (recursive workflows are not allowed).
