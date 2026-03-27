#!/bin/bash
# Monitor prover running weights vs DB CPU load
# Usage: ./monitor-provers.sh [interval_seconds]
# Default interval: 30s

INTERVAL=${1:-30}
DB_INSTANCE="zksync-mainnet2-v2-prover"
DB_PROJECT="zksync-mainnet2"
VMAUTH_URL="https://vmauth.infra.zkstack-observability.dev/select/103569:0/prometheus/api/v1/query"
VMAUTH_AUTH="admin-readonly:$(op read 'op://DevOps/Vmauth infra/password' 2>/dev/null || echo OrSq5z7VHSTpT7t2wxmE3CiAaBtlI11v)"

vm_query() {
  curl -sS -u "$VMAUTH_AUTH" "$VMAUTH_URL?query=$(python3 -c "import urllib.parse; print(urllib.parse.quote('$1'))")" 2>/dev/null
}

while true; do
  echo "=== $(date '+%H:%M:%S') ==="

  # All metrics from VictoriaMetrics in one shot
  vm_query 'autoscaler_queue{exported_job="circuit-prover-gpu"}' | python3 -c "
import sys, json
d = json.load(sys.stdin)
for r in d['data']['result']:
    ns = r['metric'].get('target_namespace','')
    val = int(r['value'][1])
    if val > 0:
        print(f'  Queue {ns}: {val:,}')
" 2>/dev/null

  vm_query 'autoscaler_target_running_weight{exported_job="circuit-prover-gpu"}' | python3 -c "
import sys, json
d = json.load(sys.stdin)
total = 0
for r in d['data']['result']:
    ns = r['metric'].get('target_namespace','')
    val = int(r['value'][1])
    total += val
    if val > 0:
        print(f'  Running weight {ns}: {val:,}')
print(f'  Running weight TOTAL: {total:,}')
" 2>/dev/null

  vm_query 'autoscaler_target_weight{exported_job="circuit-prover-gpu"}' | python3 -c "
import sys, json
d = json.load(sys.stdin)
total = 0
for r in d['data']['result']:
    ns = r['metric'].get('target_namespace','')
    val = int(r['value'][1])
    total += val
    if val > 0:
        print(f'  Desired weight {ns}: {val:,}')
print(f'  Desired weight TOTAL: {total:,}')
" 2>/dev/null

  # Per-cluster pod counts
  vm_query 'autoscaler_jobs{exported_job="circuit-prover-gpu",target_namespace="prover-red"}' | python3 -c "
import sys, json
d = json.load(sys.stdin)
for r in sorted(d['data']['result'], key=lambda x: x['metric'].get('target_cluster','')):
    cluster = r['metric'].get('target_cluster','')
    gpu = r['metric'].get('gpu','')
    val = int(r['value'][1])
    if val > 0:
        print(f'  {cluster} {gpu}: {val} pods')
" 2>/dev/null

  # DB CPU
  TOKEN=$(gcloud auth print-access-token 2>/dev/null)
  NOW=$(date -u '+%Y-%m-%dT%H:%M:%SZ')
  AGO=$(date -u -v-2M '+%Y-%m-%dT%H:%M:%SZ' 2>/dev/null || date -u -d '2 minutes ago' '+%Y-%m-%dT%H:%M:%SZ' 2>/dev/null)
  db_cpu=$(curl -s -H "Authorization: Bearer $TOKEN" \
    "https://monitoring.googleapis.com/v3/projects/${DB_PROJECT}/timeSeries?filter=metric.type%3D%22cloudsql.googleapis.com/database/cpu/utilization%22%20AND%20resource.labels.database_id%3D%22${DB_PROJECT}%3A${DB_INSTANCE}%22&interval.startTime=${AGO}&interval.endTime=${NOW}" 2>/dev/null \
    | python3 -c "import sys,json; ts=json.load(sys.stdin).get('timeSeries',[]); print(f'{ts[0][\"points\"][0][\"value\"][\"doubleValue\"]*100:.1f}%') if ts else print('N/A')" 2>/dev/null)
  echo "  DB CPU: ${db_cpu:-N/A}"

  # DB slow queries
  slow=$(gcloud logging read "resource.type=\"cloudsql_database\" AND resource.labels.database_id=\"${DB_PROJECT}:${DB_INSTANCE}\" AND textPayload=~\"duration\"" \
    --project="$DB_PROJECT" --limit=20 --freshness="${INTERVAL}s" --format="value(textPayload)" 2>/dev/null \
    | grep -o 'duration: [0-9.]* ms' | awk '{sum+=$2; n++} END {if(n>0) printf "count=%d avg=%.0fms", n, sum/n; else print "count=0"}')
  echo "  DB slow queries: $slow"

  echo
  sleep "$INTERVAL"
done
