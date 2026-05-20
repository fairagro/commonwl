The local path where the `Directory` is made available prior to executing a
`CommandLineTool`. This must be set by the implementation. This field
must not be used in any other context. The command line tool being
executed must be able to access the directory at `path` using the POSIX
`opendir(2)` syscall.

If the `path` contains [POSIX shell metacharacters](http://pubs.opengroup.org/onlinepubs/9699919799/utilities/V3_chap02.html#tag_18_02)
(`|`,`&`, `;`, `<`, `>`, `(`,`)`, `$`,`` ` ``, `\`, `"`, `'`,
`<space>`, `<tab>`, and `<newline>`) or characters
[not allowed](http://www.iana.org/assignments/idna-tables-6.3.0/idna-tables-6.3.0.xhtml)
for [Internationalized Domain Names for Applications](https://tools.ietf.org/html/rfc6452)
then implementations may terminate the process with a
`permanentFailure`.
