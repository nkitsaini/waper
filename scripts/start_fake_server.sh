#!/bin/sh
exec cargo test -- --ignored --exact 'run_server' --nocapture
