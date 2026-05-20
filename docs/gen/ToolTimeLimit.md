Set an upper limit on the execution time of a CommandLineTool.
A CommandLineTool whose execution duration exceeds the time
limit may be preemptively terminated and considered failed.
May also be used by batch systems to make scheduling decisions.
The execution duration excludes external operations, such as
staging of files, pulling a docker image etc, and only counts
wall-time for the execution of the command line itself.
