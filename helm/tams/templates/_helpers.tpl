{{/*
Expand the name of the chart.
*/}}
{{- define "tams.name" -}}
{{- .Chart.Name }}
{{- end }}

{{- define "tams.serverImage" -}}
{{ .Values.image.registry }}/{{ .Values.image.repository }}/tams-server:{{ .Values.image.tag }}
{{- end }}

{{- define "tams.authImage" -}}
{{ .Values.image.registry }}/{{ .Values.image.repository }}/tams-auth-server:{{ .Values.image.tag }}
{{- end }}

{{- define "tams.webImage" -}}
{{ .Values.image.registry }}/{{ .Values.image.repository }}/tams-web:{{ .Values.image.tag }}
{{- end }}

{{- define "tams.nodeAffinity" -}}
{{- if .Values.nodeAffinity.excludeHostname }}
affinity:
  nodeAffinity:
    requiredDuringSchedulingIgnoredDuringExecution:
      nodeSelectorTerms:
        - matchExpressions:
            - key: kubernetes.io/hostname
              operator: NotIn
              values:
                - {{ .Values.nodeAffinity.excludeHostname }}
{{- end }}
{{- end }}
