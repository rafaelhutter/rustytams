#!/usr/bin/env bash
# =============================================================================
# TAMS Kubernetes Deploy-Script
#
# Verwaltet MongoDB + TAMS Deployments im Kubernetes-Cluster.
# Nutzt CloudPirates MongoDB Helm Chart (OCI) und das lokale TAMS Helm Chart.
#
# Voraussetzungen: kubectl, helm, .env mit MONGO_PASSWORD etc.
# =============================================================================
set -euo pipefail

# --- Farben & Formatierung ---------------------------------------------------
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

# --- Konfiguration -----------------------------------------------------------
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CHART_DIR="${SCRIPT_DIR}/helm/tams"
export KUBECONFIG="/Users/r.hutter/.kube/rancher.surfplanet.yaml"

# Lade .env
if [[ -f "${SCRIPT_DIR}/.env" ]]; then
  set -a; source "${SCRIPT_DIR}/.env"; set +a
fi

# Pflicht-Defaults
NAMESPACE="${NAMESPACE:-tams}"
TAMS_USERNAME="${TAMS_USERNAME:-admin}"
TAMS_HOSTNAME="${TAMS_HOSTNAME:-tams.example.com}"

# --- Hilfsfunktionen ---------------------------------------------------------

command_exists() { command -v "$1" >/dev/null 2>&1; }

generate_password() { head /dev/urandom | tr -dc 'A-Za-z0-9' | head -c 24; }

check_deps() {
  local missing=false
  for cmd in kubectl helm; do
    if ! command_exists "$cmd"; then
      log_error "Fehlendes Tool: $cmd"
      missing=true
    fi
  done
  [[ "$missing" == "true" ]] && exit 1
  if ! kubectl version >/dev/null 2>&1; then
    log_error "Keine Verbindung zum Kubernetes-Cluster."
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
    log_error "${desc} (${var}) ist nicht gesetzt. Bitte in .env eintragen."
    exit 1
  fi
}

# --- MongoDB Deployment -------------------------------------------------------

deploy_mongodb() {
  log_section "MongoDB deployen (CloudPirates)"
  check_deps
  ensure_namespace

  require_var "MONGO_PASSWORD" "MongoDB-Passwort"

  local exclude_node="${nodeAffinity_excludeHostname:-hc-k8s-cluster-node-1}"
  # MONGO_URI für tams-server (intern im Cluster)
  MONGO_URI="mongodb://tams:${MONGO_PASSWORD}@mongodb.${NAMESPACE}.svc.cluster.local:27017/tams"

  log_info "Erstelle/aktualisiere mongodb-secret (idempotent)..."
  kubectl create secret generic mongodb-secret \
    --namespace "${NAMESPACE}" \
    --from-literal=mongodb-root-password="${MONGO_PASSWORD}" \
    --from-literal=CUSTOM_USER="tams" \
    --from-literal=CUSTOM_PASSWORD="${MONGO_PASSWORD}" \
    --from-literal=CUSTOM_DB="tams" \
    --dry-run=client -o yaml | kubectl apply -f -
  log_ok "mongodb-secret aktualisiert."

  log_info "Deploye MongoDB Helm Chart (CloudPirates OCI)..."
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
  log_ok "MongoDB Helm-Release deployt."

  log_info "Warte auf MongoDB StatefulSet rollout..."
  kubectl rollout status statefulset/mongodb \
    --namespace "${NAMESPACE}" \
    --timeout=300s
  log_ok "MongoDB ist bereit."

  # Exportiere MONGO_URI für nachfolgende deploy_tams()-Aufrufe
  export MONGO_URI
  log_info "MONGO_URI: ${MONGO_URI//:${MONGO_PASSWORD}@/:***@}"
}

# --- TAMS Deployment ----------------------------------------------------------

deploy_tams() {
  log_section "TAMS deployen (Helm)"
  check_deps
  ensure_namespace

  require_var "IMAGE_TAG"       "Image-Tag"
  require_var "B2_ACCESS_KEY"   "Backblaze B2 Access Key"
  require_var "B2_SECRET_KEY"   "Backblaze B2 Secret Key"
  require_var "TAMS_PASSWORD"   "TAMS API-Passwort"
  require_var "MONGO_PASSWORD"  "MongoDB-Passwort"
  require_var "TAMS_HOSTNAME"   "Ingress-Hostname"

  # MONGO_URI zusammenbauen (immer frisch, nicht aus Env)
  local mongo_uri="mongodb://tams:${MONGO_PASSWORD}@mongodb.${NAMESPACE}.svc.cluster.local:27017/tams"

  if [[ ! -d "${CHART_DIR}" ]]; then
    log_error "Helm Chart nicht gefunden: ${CHART_DIR}"
    exit 1
  fi

  log_info "Deploye TAMS Helm Chart (Revision: ${IMAGE_TAG})..."
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
  log_ok "TAMS Helm-Release deployt."

  log_info "Warte auf tams-server rollout..."
  kubectl rollout status deployment/tams-server \
    --namespace "${NAMESPACE}" \
    --timeout=300s
  log_ok "tams-server ist bereit."
}

# --- Verwaltungsfunktionen ---------------------------------------------------

show_status() {
  log_section "Status: Namespace '${NAMESPACE}'"
  kubectl get pods,pvc,svc,ingress \
    --namespace "${NAMESPACE}" \
    --show-labels=false 2>/dev/null || true
  echo
  log_info "Helm Releases:"
  helm list --namespace "${NAMESPACE}" 2>/dev/null || true
}

show_logs() {
  local svc="${1:-tams-server}"
  log_section "Logs: ${svc} (Namespace: ${NAMESPACE})"
  kubectl logs \
    --namespace "${NAMESPACE}" \
    --selector "app=${svc}" \
    --tail=100 \
    --follow
}

uninstall_all() {
  log_section "Deinstallation"
  echo -e "${RED}${BOLD}ACHTUNG: Alle TAMS und MongoDB Ressourcen werden gelöscht!${NC}"
  read -rp "Fortfahren? [y/N] " confirm
  [[ "${confirm:-N}" =~ ^[Yy]$ ]] || { log_info "Abgebrochen."; exit 0; }

  log_warn "Deinstalliere TAMS..."
  helm uninstall tams --namespace "${NAMESPACE}" 2>/dev/null || true

  log_warn "Deinstalliere MongoDB..."
  helm uninstall mongodb --namespace "${NAMESPACE}" 2>/dev/null || true

  log_warn "Lösche verbleibende PVCs..."
  kubectl delete pvc --all --namespace "${NAMESPACE}" 2>/dev/null || true

  log_warn "Lösche Secrets..."
  kubectl delete secret mongodb-secret tams-b2-credentials tams-credentials tams-mongo-credentials \
    --namespace "${NAMESPACE}" --ignore-not-found=true 2>/dev/null || true

  log_ok "Deinstallation abgeschlossen."
}

cleanup_pvc() {
  log_section "PVC cleanup: tams-server-data"
  echo -e "${YELLOW}Dieser Befehl löscht den tams-server-data PVC (JSON-File-Daten).${NC}"
  echo -e "${YELLOW}Nur ausführen, nachdem MongoDB-Migration verifiziert wurde!${NC}"
  read -rp "Fortfahren? [y/N] " confirm
  [[ "${confirm:-N}" =~ ^[Yy]$ ]] || { log_info "Abgebrochen."; exit 0; }

  kubectl delete pvc tams-server-data \
    --namespace "${NAMESPACE}" \
    --ignore-not-found=true
  log_ok "tams-server-data PVC gelöscht."
  log_info "Deploye TAMS ohne PVC (helm upgrade)..."
  deploy_tams
}

usage() {
  cat <<EOF

${BOLD}TAMS Deploy-Script${NC}

${BOLD}Verwendung:${NC}
  $(basename "$0") <Befehl> [Optionen]

${BOLD}Befehle:${NC}
  all          MongoDB + TAMS deployen (komplettes Deployment)
  mongodb      Nur MongoDB deployen/aktualisieren
  tams         Nur TAMS deployen/aktualisieren
  status       Status aller Ressourcen im Namespace anzeigen
  logs [svc]   Logs folgen (default: tams-server)
  uninstall    Alle Ressourcen löschen (mit Bestätigung)
  cleanup-pvc  tams-server-data PVC löschen (nach MongoDB-Migration)

${BOLD}Konfiguration (.env):${NC}
  MONGO_PASSWORD   MongoDB-Passwort (Pflicht)
  IMAGE_TAG        Docker Image-Tag
  B2_ACCESS_KEY    Backblaze B2 Access Key
  B2_SECRET_KEY    Backblaze B2 Secret Key
  TAMS_PASSWORD    TAMS API-Passwort
  TAMS_HOSTNAME    Ingress-Hostname
  NAMESPACE        Kubernetes-Namespace (default: tams)

${BOLD}Beispiele:${NC}
  ./$(basename "$0") all          # Vollständiges Deployment
  ./$(basename "$0") mongodb      # Nur MongoDB
  ./$(basename "$0") tams         # Nur TAMS (MongoDB muss laufen)
  ./$(basename "$0") logs         # tams-server Logs
  ./$(basename "$0") logs mongodb # MongoDB Logs
  ./$(basename "$0") status       # Cluster-Status
EOF
  exit 1
}

# --- Haupt-Logik -------------------------------------------------------------
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
      log_error "Unbekannter Befehl: $cmd"
      usage
      ;;
  esac
}

main "$@"
