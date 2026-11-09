{{/* The labels of each object. */}}
{{- define "observer.labels" -}}
app.kubernetes.io/name: strata-observer
app.kubernetes.io/version: {{ .Chart.AppVersion | quote }}
app.kubernetes.io/managed-by: {{ .Release.Service }}
{{- end -}}

{{/* The image, with the version of the chart as the default tag. */}}
{{- define "observer.image" -}}
{{ .Values.image.repository }}:{{ .Values.image.tag | default (printf "v%s" .Chart.AppVersion) }}
{{- end -}}

{{/* The environment of the observer and of the seed: the settings, and the key from its Secret. */}}
{{- define "observer.env" -}}
- name: STRATA_CLUSTER_UID
  valueFrom: { configMapKeyRef: { name: strata-observer, key: cluster-uid } }
- name: PALETTE_API
  valueFrom: { configMapKeyRef: { name: strata-observer, key: palette-api } }
- name: PALETTE_PROJECT_UID
  valueFrom: { configMapKeyRef: { name: strata-observer, key: project-uid } }
- name: PALETTE_API_KEY
  valueFrom: { secretKeyRef: { name: {{ .Values.palette.keySecret }}, key: api-key } }
- name: HOME
  value: /work
- name: TMPDIR
  value: /work
{{- end -}}

{{/* The security of a container under the Pod Security standard restricted. */}}
{{- define "observer.containerSecurity" -}}
allowPrivilegeEscalation: false
readOnlyRootFilesystem: true
capabilities:
  drop: [ALL]
{{- end -}}

{{- define "observer.podSecurity" -}}
runAsNonRoot: true
runAsUser: 65532
runAsGroup: 65532
fsGroup: 65532
seccompProfile:
  type: RuntimeDefault
{{- end -}}

{{- define "observer.pullSecrets" -}}
{{- if .Values.image.pullSecret }}
imagePullSecrets:
  - name: {{ .Values.image.pullSecret }}
{{- end }}
{{- end -}}
