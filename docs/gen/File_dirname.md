The name of the directory containing file, that is, the path leading up
to the final slash in the path such that `dirname + '/' + basename ==
path`.

The implementation must set this field based on the value of `path`
prior to evaluating parameter references or expressions in a
CommandLineTool document.  This field must not be used in any other
context.
