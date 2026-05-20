Minimum reserved number of CPU cores (default is 1).

May be a fractional value to indicate to a scheduling
algorithm that one core can be allocated to multiple
jobs. For example, a value of 0.25 indicates that up to 4
jobs may run in parallel on 1 core.  A value of 1.25 means
that up to 3 jobs can run on a 4 core system (4/1.25 ≈ 3).

Processes can only share a core allocation if the sum of each
of their `ramMax`, `tmpdirMax`, and `outdirMax` requests also
do not exceed the capacity of the node.

Processes sharing a core must have the same level of isolation
(typically a container or VM) that they would normally have.

The reported number of CPU cores reserved for the process,
which is available to expressions on the CommandLineTool as
`runtime.cores`, must be a non-zero integer, and may be
calculated by rounding up the cores request to the next whole
number.

Scheduling systems may allocate fractional CPU resources by
setting quotas or scheduling weights.  Scheduling systems that
do not support fractional CPUs may round up the request to the
next whole number.
