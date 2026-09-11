#!/usr/bin/env bash
# Provision canonical workspace databases per ENVIRONMENT_SPEC §7.1
# in the existing docker postgres container (root/123456).
set -e

PG_CONTAINER="${1:-postgres}"
PG_USER="${2:-root}"

run_sql() {
  docker exec "${PG_CONTAINER}" psql -U "${PG_USER}" -d postgres -c "$1"
}

run_sql_db() {
  docker exec "${PG_CONTAINER}" psql -U "${PG_USER}" -d "$1" -c "$2"
}

# Create roles
run_sql "DO \$\$BEGIN IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname='sdkwork_ai_dev') THEN CREATE ROLE sdkwork_ai_dev LOGIN PASSWORD 'sdkworkdev123'; END IF; END\$\$;"
run_sql "DO \$\$BEGIN IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname='sdkwork_ai_test') THEN CREATE ROLE sdkwork_ai_test LOGIN PASSWORD 'sdkworktest123'; END IF; END\$\$;"
run_sql "DO \$\$BEGIN IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname='sdkwork_ai_prod') THEN CREATE ROLE sdkwork_ai_prod LOGIN PASSWORD 'sdkworkprod123'; END IF; END\$\$;"

# Create databases
run_sql "DO \$\$BEGIN IF NOT EXISTS (SELECT FROM pg_database WHERE datname='sdkwork_ai_dev') THEN PERFORM dblink_exec('dbname=postgres','CREATE DATABASE sdkwork_ai_dev OWNER sdkwork_ai_dev'); END IF; END\$\$;" 2>/dev/null || \
  docker exec "${PG_CONTAINER}" psql -U "${PG_USER}" -d postgres -tc "SELECT 1 FROM pg_database WHERE datname='sdkwork_ai_dev'" | grep -q 1 || \
  run_sql "CREATE DATABASE sdkwork_ai_dev OWNER sdkwork_ai_dev;"

docker exec "${PG_CONTAINER}" psql -U "${PG_USER}" -d postgres -tc "SELECT 1 FROM pg_database WHERE datname='sdkwork_ai_test'" | grep -q 1 || \
  run_sql "CREATE DATABASE sdkwork_ai_test OWNER sdkwork_ai_test;"

docker exec "${PG_CONTAINER}" psql -U "${PG_USER}" -d postgres -tc "SELECT 1 FROM pg_database WHERE datname='sdkwork_ai_prod'" | grep -q 1 || \
  run_sql "CREATE DATABASE sdkwork_ai_prod OWNER sdkwork_ai_prod;"

# Create schemas and set search_path
for pair in "sdkwork_ai_dev sdkwork_ai_dev" "sdkwork_ai_test sdkwork_ai_test" "sdkwork_ai_prod sdkwork_ai_prod"; do
  db=$(echo "$pair" | cut -d' ' -f1)
  user=$(echo "$pair" | cut -d' ' -f2)
  run_sql_db "$db" "CREATE SCHEMA IF NOT EXISTS ${db}; GRANT ALL ON SCHEMA ${db} TO ${user}; GRANT CREATE ON SCHEMA ${db} TO ${user}; ALTER ROLE ${user} SET search_path TO ${db};"
done

echo "Done: databases sdkwork_ai_dev, sdkwork_ai_test, sdkwork_ai_prod provisioned."
