Defines the input parameters of the process.  The process is ready to
run when all required input parameters are associated with concrete
values.  Input parameters include a schema for each parameter which is
used to validate the input object.  It may also be used to build a user
interface for constructing the input object.

When accepting an input object, all input parameters must have a value.
If an input parameter is missing from the input object, it must be
assigned a value of `null` (or the value of `default` for that
parameter, if provided) for the purposes of validation and evaluation
of expressions.
