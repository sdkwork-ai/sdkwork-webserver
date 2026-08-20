#!/usr/bin/env bash
# Provision sdkwork-specs-compliant databases in an external PostgreSQL instance.
# Usage: docker exec postgres bash /provision.sh
#   or:  bash provision-external-postgres.sh  (when run inside the PG container)
set -euo pipefail

PG_SUPERUSER="${PG_SUPERUSER:-root}"

create_identity() {
  local db="$1" user="$2" pass="$3"
  psql -U "${PG_SUPERUSER}" -d postgres <<EOSQL
DO \$\$
BEGIN
  IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = '${user}') THEN
    EXECUTE format('CREATE ROLE %I LOGIN PASSWORD %L', '${user}', '${pass}');
  ELSE
    EXECUTE format('ALTER ROLE %I WITH LOGIN PASSWORD %L', '${user}', '${pass}');
  END IF;
END\$\$;
EOSQL

  psql -U "${PG_SUPERUSER}" -d postgres -tc "SELECT 1 FROM pg_database WHERE datname = '${db}'" | grep -q 1 \
    || psql -U "${PG_SUPERUSER}" -d postgres -c "CREATE DATABASE \"${db}\" OWNER \"${user}\";"

  psql -U "${PG_SUPERUSER}" -d postgres -c "GRANT ALL PRIVILEGES ON DATABASE \"${db}\" TO \"${user}\";"

  psql -U "${PG_SUPERUSER}" -d "${db}" <<EOSQL
CREATE SCHEMA IF NOT EXISTS "${db}";
GRANT ALL ON SCHEMA "${db}" TO "${user}";
GRANT CREATE ON SCHEMA "${db}" TO "${user}";
ALTER ROLE "${user}" SET search_path TO "${db}";
EOSQL
  echo "provisioned: db=${db} user=${user} schema=${db}"
}

create_identity "sdkwork_ai_dev"  "sdkwork_ai_dev"  "sdkworkdev123"
create_identity "sdkwork_ai_test" "sdkwork_ai_test"  "sdkworktest123"
create_identity "sdkwork_ai_prod" "sdkwork_ai_prod"  "sdkworkprod123"

echo "all workspace database identities provisioned"
