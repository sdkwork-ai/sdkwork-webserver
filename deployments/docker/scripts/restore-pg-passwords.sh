#!/usr/bin/env bash
# Restore PostgreSQL passwords to match sdkwork-cloudrouter spec
set -euo pipefail

echo "Restoring PostgreSQL passwords..."

# Restore sdkwork_ai_dev
su - postgres -c "psql -c \"ALTER USER sdkwork_ai_dev WITH PASSWORD 'sdkworkdev123';\""
echo "  sdkwork_ai_dev password restored"

# Restore sdkwork_ai_test
su - postgres -c "psql -c \"ALTER USER sdkwork_ai_test WITH PASSWORD 'sdkworktest123';\""
echo "  sdkwork_ai_test password restored"

# Restore sdkwork_ai_prod
su - postgres -c "psql -c \"ALTER USER sdkwork_ai_prod WITH PASSWORD 'sdkworkprod123';\""
echo "  sdkwork_ai_prod password restored"

# Restore postgres superuser
su - postgres -c "psql -c \"ALTER USER postgres WITH PASSWORD 'postgres_admin_pass';\""
echo "  postgres superuser password restored"

echo ""
echo "Verifying passwords..."
PGPASSWORD=sdkworkdev123 psql -h 127.0.0.1 -U sdkwork_ai_dev -d sdkwork_ai_dev -tAc "SELECT 'dev OK' AS status;" 2>/dev/null || echo "  dev: FAILED"
PGPASSWORD=sdkworktest123 psql -h 127.0.0.1 -U sdkwork_ai_test -d sdkwork_ai_test -tAc "SELECT 'test OK' AS status;" 2>/dev/null || echo "  test: FAILED"
PGPASSWORD=sdkworkprod123 psql -h 127.0.0.1 -U sdkwork_ai_prod -d sdkwork_ai_prod -tAc "SELECT 'prod OK' AS status;" 2>/dev/null || echo "  prod: FAILED"

echo ""
echo "Done!"
