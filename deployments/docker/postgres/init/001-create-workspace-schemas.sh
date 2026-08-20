#!/bin/sh
# sdkwork-webserver standalone compose — built-in PostgreSQL bootstrap.
# Provisions canonical workspace identities (ENVIRONMENT_SPEC §7.1).
set -e

create_identity() {
  db_name="$1"
  db_user="$2"
  db_password="$3"

  # 1) Role create/update (allowed inside DO blocks).
  psql -v ON_ERROR_STOP=1 --username "${POSTGRES_USER}" --dbname "${POSTGRES_DB}" <<-EOSQL
DO \$\$
BEGIN
  IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = '${db_user}') THEN
    EXECUTE format('CREATE ROLE %I LOGIN PASSWORD %L', '${db_user}', '${db_password}');
  ELSE
    EXECUTE format('ALTER ROLE %I WITH LOGIN PASSWORD %L', '${db_user}', '${db_password}');
  END IF;
END
\$\$;
EOSQL

  # 2) Database create/update (MUST be outside DO — CREATE DATABASE is not
  # allowed from within a function-like context).
  db_exists="$(psql -v ON_ERROR_STOP=1 --username "${POSTGRES_USER}" --dbname "${POSTGRES_DB}" \
    -tAc "SELECT 1 FROM pg_database WHERE datname = '${db_name}'" | tr -d '[:space:]' || true)"
  if [ "${db_exists}" != "1" ]; then
    psql -v ON_ERROR_STOP=1 --username "${POSTGRES_USER}" --dbname "${POSTGRES_DB}" \
      -c "CREATE DATABASE \"${db_name}\" OWNER \"${db_user}\";"
  else
    psql -v ON_ERROR_STOP=1 --username "${POSTGRES_USER}" --dbname "${POSTGRES_DB}" \
      -c "ALTER DATABASE \"${db_name}\" OWNER TO \"${db_user}\";"
  fi
  psql -v ON_ERROR_STOP=1 --username "${POSTGRES_USER}" --dbname "${POSTGRES_DB}" \
    -c "GRANT ALL PRIVILEGES ON DATABASE \"${db_name}\" TO \"${db_user}\";"

  # 3) Schema & search_path (executed inside the database).
  psql -v ON_ERROR_STOP=1 --username "${POSTGRES_USER}" --dbname "${db_name}" <<-EOSQL
CREATE SCHEMA IF NOT EXISTS "${db_name}";
GRANT ALL ON SCHEMA "${db_name}" TO "${db_user}";
GRANT CREATE ON SCHEMA "${db_name}" TO "${db_user}";
ALTER ROLE "${db_user}" SET search_path TO "${db_name}";
EOSQL
}

create_identity \
  "${WEBSERVER_POSTGRES_DEV_DB:-sdkwork_ai_dev}" \
  "${WEBSERVER_POSTGRES_DEV_USER:-sdkwork_ai_dev}" \
  "${WEBSERVER_POSTGRES_DEV_PASSWORD:-sdkworkdev123}"

create_identity \
  "${WEBSERVER_POSTGRES_TEST_DB:-sdkwork_ai_test}" \
  "${WEBSERVER_POSTGRES_TEST_USER:-sdkwork_ai_test}" \
  "${WEBSERVER_POSTGRES_TEST_PASSWORD:-sdkworktest123}"

create_identity \
  "${WEBSERVER_POSTGRES_PROD_DB:-sdkwork_ai_prod}" \
  "${WEBSERVER_POSTGRES_PROD_USER:-sdkwork_ai_prod}" \
  "${WEBSERVER_POSTGRES_PROD_PASSWORD:-sdkworkprod123}"
