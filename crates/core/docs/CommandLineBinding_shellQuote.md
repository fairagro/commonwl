If `ShellCommandRequirement` is in the requirements for the current command,
this controls whether the value is quoted on the command line (default is true).
Use `shellQuote: false` to inject metacharacters for operations such as pipes.

If `shellQuote` is true or not provided, the implementation must not
permit interpretation of any shell metacharacters or directives.
