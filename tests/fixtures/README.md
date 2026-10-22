# Fixtures

What the real install script printed, for the contract tests in `../contract.rs`.

They were captured by running `tools/install/nyra-install.sh` from the nyra-base repository
(commit `e2c9414acd770224dba63a768f07f4f730a05f44`, the machine interface of protocol version 1)
with the stand-ins for the machine and the steps of the image that its own interface test uses
(`tools/install/test-machine-interface.sh`):

- `check-*.json`: the output of `check` on several machines. `disks-*.json`: the output of `disks`.
- `*.events` and `*.exit`: the events of one run of `install` or `mok-import` (the file written on
  descriptor 3) and its exit status.

They are not edited by hand. When the interface changes, capture them again from the new script.
