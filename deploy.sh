#!/usr/bin/env bash
# =============================================================================
# TAMS Kubernetes Deploy Script
#
# Manages MongoDB + TAMS deployments in the Kubernetes cluster.
# Uses the CloudPirates MongoDB Helm Chart (OCI) and the local TAMS Helm Chart.
#
# Prerequisites: kubectl, helm, .env with MONGO_PASSWORD etc.
# =============================================================================
set -euo pipefail

# --- Colors & formatting -----------------------------------------------------
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
BOLD='\033[1m'
NC='\033[0m'

log_info()    { echo -e "${BLUE}[INFO]${NC}  $*"; }
log_ok()      { echo -e "${GREEN}[OK]${NC}    $*"; }
log_warn()    { echo -e "${YELLOW}[WARN]${NC}  $*"; }
log_error()   { echo -e "${RED}[ERROR]${NC} $*" >&2; }
log_section() { echo -e "\n${BOLD}━━━ $* ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"; }

# --- Configuration -----------------------------------------------------------
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CHART_DIR="${SCRIPT_DIR}/helm/tams"
export KUBECONFIG="/Users/r.hutter/.kube/rancher.surfplanet.yaml"

# Load .env
if [[ -f "${SCRIPT_DIR}/.env" ]]; then
  set -a; source "${SCRIPT_DIR}/.env"; set +a
fi

# Defaults
NAMESPACE="${NAMESPACE:-tams}"
TAMS_USERNAME="${TAMS_USERNAME:-admin}"
TAMS_HOSTNAME="${TAMS_HOSTNAME:-tams.example.com}"

# --- Helper functions --------------------------------------------------------

command_exists() { command -v "$1" >/dev/null 2>&1; }

generate_password() { head /dev/urandom | tr -dc 'A-Za-z0-9' | head -c 24; }

check_deps() {
  local missing=false
  for cmd in kubectl helm; do
    if ! command_exists "$cmd"; then
      log_error "Missing tool: $cmd"
      missing=true
    fi
  done
  [[ "$missing" == "true" ]] && exit 1
  if ! kubectl version >/dev/null 2>&1; then
    log_error "Cannot connect to Kubernetes cluster."
    exit 1
  fi
}

ensure_namespace() {
  kubectl get namespace "${NAMESPACE}" >/dev/null 2>&1 || \
    kubectl create namespace "${NAMESPACE}"
  log_ok "Namespace '${NAMESPACE}' vorhanden."
}

require_var() {
  local var="$1" desc="$2"
  if [[ -z "${!var:-}" ]]; then
    log_error "${desc} (${var}) is not set. Please add it to .env."
    exit 1
  fi
}

# --- MongoDB deployment -------------------------------------------------------

deploy_mongodb() {
  log_section "Deploy MongoDB (CloudPirates)"
  check_deps
  ensure_namespace

  require_var "MONGO_PASSWORD" "MongoDB password"

  local exclude_node="${nodeAffinity_excludeHostname:-hc-k8s-cluster-node-1}"
  # MONGO_URI for tams-server (cluster-internal)
  MONGO_URI="mongodb://tams:${MONGO_PASSWORD}@mongodb.${NAMESPACE}.svc.cluster.local:27017/tams"

  log_info "Creating/updating mongodb-secret (idempotent)..."
  kubectl create secret generic mongodb-secret \
    --namespace "${NAMESPACE}" \
    --from-literal=mongodb-root-password="${MONGO_PASSWORD}" \
    --from-literal=CUSTOM_USER="tams" \
    --from-literal=CUSTOM_PASSWORD="${MONGO_PASSWORD}" \
    --from-literal=CUSTOM_DB="tams" \
    --dry-run=client -o yaml | kubectl apply -f -
  log_ok "mongodb-secret updated."

  log_info "Deploying MongoDB Helm Chart (CloudPirates OCI)..."
  helm upgrade --install mongodb \
    oci://registry-1.docker.io/cloudpirates/mongodb \
    --namespace "${NAMESPACE}" \
    --set "auth.enabled=true" \
    --set "auth.rootUsername=admin" \
    --set "auth.existingSecret=mongodb-secret" \
    --set "auth.existingSecretPasswordKey=mongodb-root-password" \
    --set "customUsers[0].existingSecret=mongodb-secret" \
    --set "customUsers[0].secretKeys.name=CUSTOM_USER" \
    --set "customUsers[0].secretKeys.password=CUSTOM_PASSWORD" \
    --set "customUsers[0].secretKeys.database=CUSTOM_DB" \
    --set "customUsers[0].roles[0]=readWrite" \
    --set "customUsers[0].roles[1]=dbAdmin" \
    --set "persistence.enabled=true" \
    --set "persistence.storageClass=longhorn" \
    --set "persistence.size=2Gi" \
    --set "affinity.nodeAffinity.requiredDuringSchedulingIgnoredDuringExecution.nodeSelectorTerms[0].matchExpressions[0].key=kubernetes.io/hostname" \
    --set "affinity.nodeAffinity.requiredDuringSchedulingIgnoredDuringExecution.nodeSelectorTerms[0].matchExpressions[0].operator=NotIn" \
    --set "affinity.nodeAffinity.requiredDuringSchedulingIgnoredDuringExecution.nodeSelectorTerms[0].matchExpressions[0].values[0]=${exclude_node}" \
    --timeout 300s
  log_ok "MongoDB Helm release deployed."

  log_info "Waiting for MongoDB StatefulSet rollout..."
  kubectl rollout status statefulset/mongodb \
    --namespace "${NAMESPACE}" \
    --timeout=300s
  log_ok "MongoDB is ready."

  # Export MONGO_URI for subsequent deploy_tams() calls
  export MONGO_URI
  log_info "MONGO_URI: ${MONGO_URI//:${MONGO_PASSWORD}@/:***@}"
}

# --- TAMS deployment ----------------------------------------------------------

deploy_tams() {
  log_section "Deploy TAMS (Helm)"
  check_deps
  ensure_namespace

  require_var "IMAGE_TAG"       "Image tag"
  require_var "B2_ACCESS_KEY"   "Backblaze B2 access key"
  require_var "B2_SECRET_KEY"   "Backblaze B2 secret key"
  require_var "TAMS_PASSWORD"   "TAMS API password"
  require_var "MONGO_PASSWORD"  "MongoDB password"
  require_var "TAMS_HOSTNAME"   "Ingress hostname"

  # Build MONGO_URI fresh (never rely on env)
  local mongo_uri="mongodb://tams:${MONGO_PASSWORD}@mongodb.${NAMESPACE}.svc.cluster.local:27017/tams"

  if [[ ! -d "${CHART_DIR}" ]]; then
    log_error "Helm chart not found: ${CHART_DIR}"
    exit 1
  fi

  log_info "Deploying TAMS Helm Chart (revision: ${IMAGE_TAG})..."
  helm upgrade --install tams "${CHART_DIR}" \
    --namespace "${NAMESPACE}" \
    --values "${CHART_DIR}/values.yaml" \
    --values "${CHART_DIR}/values-prod.yaml" \
    --set "image.tag=${IMAGE_TAG}" \
    --set "b2.endpoint=${B2_ENDPOINT:-s3.eu-central-003.backblazeb2.com}" \
    --set "b2.region=${B2_REGION:-eu-central-003}" \
    --set "b2.bucket=${B2_BUCKET:-tams-media}" \
    --set "b2.accessKey=${B2_ACCESS_KEY}" \
    --set "b2.secretKey=${B2_SECRET_KEY}" \
    --set "tams.username=${TAMS_USERNAME}" \
    --set "tams.password=${TAMS_PASSWORD}" \
    --set "ingress.host=${TAMS_HOSTNAME}" \
    --set "mongoUri=${mongo_uri}" \
    --timeout 300s
  log_ok "TAMS Helm release deployed."

  log_info "Waiting for tams-server rollout..."
  kubectl rollout status deployment/tams-server \
    --namespace "${NAMESPACE}" \
    --timeout=300s
  log_ok "tams-server is ready."
}

# --- Management functions ----------------------------------------------------

show_status() {
  log_section "Status: namespace '${NAMESPACE}'"
  kubectl get pods,pvc,svc,ingress \
    --namespace "${NAMESPACE}" \
    --show-labels=false 2>/dev/null || true
  echo
  log_info "Helm Releases:"
  helm list --namespace "${NAMESPACE}" 2>/dev/null || true
}

show_logs() {
  local svc="${1:-tams-server}"
  log_section "Logs: ${svc} (namespace: ${NAMESPACE})"
  kubectl logs \
    --namespace "${NAMESPACE}" \
    --selector "app=${svc}" \
    --tail=100 \
    --follow
}

uninstall_all() {
  log_section "Uninstall"
  echo -e "${RED}${BOLD}WARNING: All TAMS and MongoDB resources will be deleted!${NC}"
  read -rp "Continue? [y/N] " confirm
  [[ "${confirm:-N}" =~ ^[Yy]$ ]] || { log_info "Aborted."; exit 0; }

  log_warn "Uninstalling TAMS..."
  helm uninstall tams --namespace "${NAMESPACE}" 2>/dev/null || true

  log_warn "Uninstalling MongoDB..."
  helm uninstall mongodb --namespace "${NAMESPACE}" 2>/dev/null || true

  log_warn "Deleting remaining PVCs..."
  kubectl delete pvc --all --namespace "${NAMESPACE}" 2>/dev/null || true

  log_warn "Deleting secrets..."
  kubectl delete secret mongodb-secret tams-b2-credentials tams-credentials tams-mongo-credentials \
    --namespace "${NAMESPACE}" --ignore-not-found=true 2>/dev/null || true

  log_ok "Uninstall complete."
}

cleanup_pvc() {
  log_section "PVC cleanup: tams-server-data"
  echo -e "${YELLOW}This will delete the tams-server-data PVC (JSON file data).${NC}"
  echo -e "${YELLOW}Only run this after verifying the MongoDB migration!${NC}"
  read -rp "Continue? [y/N] " confirm
  [[ "${confirm:-N}" =~ ^[Yy]$ ]] || { log_info "Aborted."; exit 0; }

  kubectl delete pvc tams-server-data \
    --namespace "${NAMESPACE}" \
    --ignore-not-found=true
  log_ok "tams-server-data PVC deleted."
  log_info "Deploying TAMS without PVC (helm upgrade)..."
  deploy_tams
}

usage() {
  cat <<EOF

${BOLD}TAMS Deploy Script${NC}

${BOLD}Usage:${NC}
  $(basename "$0") <command> [options]

${BOLD}Commands:${NC}
  all          Deploy MongoDB + TAMS (full deployment)
  mongodb      Deploy/update MongoDB only
  tams         Deploy/update TAMS only (MongoDB must be running)
  status       Show status of all resources in the namespace
  logs [svc]   Follow logs (default: tams-server)
  uninstall    Delete all resources (with confirmation)
  cleanup-pvc  Delete tams-server-data PVC (after MongoDB migration)

${BOLD}Configuration (.env):${NC}
  MONGO_PASSWORD   MongoDB password (required)
  IMAGE_TAG        Docker image tag
  B2_ACCESS_KEY    Backblaze B2 access key
  B2_SECRET_KEY    Backblaze B2 secret key
  TAMS_PASSWORD    TAMS API password
  TAMS_HOSTNAME    Ingress hostname
  NAMESPACE        Kubernetes namespace (default: tams)

${BOLD}Examples:${NC}
  ./$(basename "$0") all          # Full deployment
  ./$(basename "$0") mongodb      # MongoDB only
  ./$(basename "$0") tams         # TAMS only (MongoDB must be running)
  ./$(basename "$0") logs         # tams-server logs
  ./$(basename "$0") logs mongodb # MongoDB logs
  ./$(basename "$0") status       # Cluster status
EOF
  exit 1
}

# --- Main logic --------------------------------------------------------------
main() {
  [[ $# -eq 0 ]] && usage

  local cmd="$1"; shift

  case "$cmd" in
    all)
      deploy_mongodb
      deploy_tams
      show_status
      ;;
    mongodb)
      deploy_mongodb
      show_status
      ;;
    tams)
      deploy_tams
      show_status
      ;;
    status)
      show_status
      ;;
    logs)
      show_logs "${1:-tams-server}"
      ;;
    uninstall)
      uninstall_all
      ;;
    cleanup-pvc)
      cleanup_pvc
      ;;
    *)
      log_error "Unknown command: $cmd"
      usage
      ;;
  esac
}

main "$@"
