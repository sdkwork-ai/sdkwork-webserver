#!/bin/bash
su - postgres -c "psql -qc \"ALTER ROLE sdkwork_ai_demo PASSWORD 'sdkworkdemo123'\"" >/dev/null
for i in $(seq 1 14); do
  ts=$(date +%H:%M:%S)
  if PGPASSWORD=sdkworkdemo123 timeout 5 psql -h 127.0.0.1 -U sdkwork_ai_demo -d sdkwork_ai_demo -tAc "SELECT 1" >/dev/null 2>&1; then
    echo "$ts OK"
  else
    echo "$ts FAIL"
  fi
  sleep 10
done
