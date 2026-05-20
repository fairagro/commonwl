The basename root such that `nameroot + nameext == basename`, and
`nameext` is empty or begins with a period and contains at most one
period. For the purposes of path splitting leading periods on the
basename are ignored; a basename of `.cshrc` will have a nameroot of
`.cshrc`.

The implementation must set this field automatically based on the value
of `basename` prior to evaluating parameter references or expressions.
