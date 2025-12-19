#!/bin/bash
wrk -t4 -c100 -d30s -s benchmark.lua http://localhost:3000

