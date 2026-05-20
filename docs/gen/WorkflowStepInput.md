The input of a workflow step connects an upstream parameter (from the
workflow inputs, or the outputs of other workflows steps) with the input
parameters of the process specified by the `run` field. Only input parameters
declared by the target process will be passed through at runtime to the process
though additional parameters may be specified (for use within `valueFrom`
expressions for instance) - unconnected or unused parameters do not represent an
error condition.

# Input object

A WorkflowStepInput object must contain an `id` field in the form
`#fieldname` or `#prefix/fieldname`.  When the `id` field contains a slash
`/` the field name consists of the characters following the final slash
(the prefix portion may contain one or more slashes to indicate scope).
This defines a field of the workflow step input object with the value of
the `source` parameter(s).

# Merging multiple inbound data links

To merge multiple inbound data links,
[MultipleInputFeatureRequirement](#MultipleInputFeatureRequirement) must be specified
in the workflow or workflow step requirements.

If the sink parameter is an array, or named in a [workflow
scatter](#WorkflowStep) operation, there may be multiple inbound
data links listed in the `source` field.  The values from the
input links are merged depending on the method specified in the
`linkMerge` field.  If both `linkMerge` and `pickValue` are null
or not specified, and there is more than one element in the
`source` array, the default method is "merge_nested".

If both `linkMerge` and `pickValue` are null or not specified, and
there is only a single element in the `source`, then the input
parameter takes the scalar value from the single input link (it is
*not* wrapped in a single-list).

* **merge_nested**

  The input must be an array consisting of exactly one entry for each
  input link.  If "merge_nested" is specified with a single link, the value
  from the link must be wrapped in a single-item list.

* **merge_flattened**

  1. The source and sink parameters must be compatible types, or the source
     type must be compatible with single element from the "items" type of
     the destination array parameter.
  2. Source parameters which are arrays are concatenated.
     Source parameters which are single element types are appended as
     single elements.

# Picking non-null values among inbound data links

If present, `pickValue` specifies how to pick non-null values among inbound data links.

`pickValue` is evaluated
  1. Once all source values from upstream step or parameters are available.
  2. After `linkMerge`.
  3. Before `scatter` or `valueFrom`.

This is specifically intended to be useful in combination with
[conditional execution](#WorkflowStep), where several upstream
steps may be connected to a single input (`source` is a list), and
skipped steps produce null values.

Static type checkers should check for type consistency after inferring what the type
will be after `pickValue` is applied, just as they do currently for `linkMerge`.

* **first_non_null**

  For the first level of a list input, pick the first non-null element.  The result is a scalar.
  It is an error if there is no non-null element.  Examples:
  * `[null, x, null, y] -> x`
  * `[null, [null], null, y] -> [null]`
  * `[null, null, null] -> Runtime Error`

  *Intended use case*: If-else pattern where the
  value comes either from a conditional step or from a default or
  fallback value. The conditional step(s) should be placed first in
  the list.

* **the_only_non_null**

  For the first level of a list input, pick the single non-null element.  The result is a scalar.
  It is an error if there is more than one non-null element.  Examples:

  * `[null, x, null] -> x`
  * `[null, x, null, y] -> Runtime Error`
  * `[null, [null], null] -> [null]`
  * `[null, null, null] -> Runtime Error`

  *Intended use case*: Switch type patterns where developer considers
  more than one active code path as a workflow error
  (possibly indicating an error in writing `when` condition expressions).

* **all_non_null**

  For the first level of a list input, pick all non-null values.
  The result is a list, which may be empty.  Examples:

  * `[null, x, null] -> [x]`
  * `[x, null, y] -> [x, y]`
  * `[null, [x], [null]] -> [[x], [null]]`
  * `[null, null, null] -> []`

  *Intended use case*: It is valid to have more than one source, but
   sources are conditional, so null sources (from skipped steps)
   should be filtered out.
