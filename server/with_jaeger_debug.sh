#!/bin/bash

set -m
QUERY_BASE_PATH=/admin/jaeger ../../jaeger/jaeger-all-in-one > /dev/null 2>& 1 &
set +m

trap "kill -- -$!" EXIT

eval "$@"
