The basename extension such that `nameroot + nameext == basename`, and
`nameext` is empty or begins with a period and contains at most one
period.  Leading periods on the basename are ignored; a basename of
`.cshrc` will have an empty `nameext`.

The implementation must set this field automatically based on the value
of `basename` prior to evaluating parameter references or expressions.
