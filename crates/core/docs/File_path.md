The local host path where the `File` is available when a `CommandLineTool` is
executed. This field must be set by the implementation. The final
path component must match the value of `basename`. This field
must not be used in any other context. The command line tool being
executed must be able to access the file at `path` using the POSIX
`open(2)` syscall.

As a special case, if the `path` field is provided but the `location`
field is not, an implementation may assign the value of the `path`
field to `location`, and remove the `path` field.

If the `path` contains [POSIX shell metacharacters](http://pubs.opengroup.org/onlinepubs/9699919799/utilities/V3_chap02.html#tag_18_02)
(`|`,`&`, `;`, `<`, `>`, `(`,`)`, `$`,`` ` ``, `\`, `"`, `'`,
`<space>`, `<tab>`, and `<newline>`) or characters
[not allowed](http://www.iana.org/assignments/idna-tables-6.3.0/idna-tables-6.3.0.xhtml)
for [Internationalized Domain Names for Applications](https://tools.ietf.org/html/rfc6452)
then implementations may terminate the process with a
`permanentFailure`.
