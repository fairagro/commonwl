For implementations that support reusing output from past work (on
the assumption that same code and same input produce same
results), control whether to enable or disable the reuse behavior
for a particular tool or step (to accommodate situations where that
assumption is incorrect).  A reused step is not executed but
instead returns the same output as the original execution.

If `WorkReuse` is not specified, correct tools should assume it
is enabled by default.
