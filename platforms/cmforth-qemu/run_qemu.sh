#!/usr/bin/env bash

BIN="$1"
shift

exec qemu-system-arm -cpu cortex-m7 -machine mps2-an500 -kernel "${BIN}" -display none -semihosting-config enable=true,chardev=cid -chardev stdio,id=cid $@
