{{/*
Common naming and label helpers.
*/}}

{{- define "forge.name" -}}
{{- default .Chart.Name .Values.nameOverride | trunc 63 | trimSuffix "-" -}}
{{- end -}}

{{- define "forge.fullname" -}}
{{- if .Values.fullnameOverride -}}
{{- .Values.fullnameOverride | trunc 63 | trimSuffix "-" -}}
{{- else -}}
{{- $name := default .Chart.Name .Values.nameOverride -}}
{{- if contains $name .Release.Name -}}
{{- .Release.Name | trunc 63 | trimSuffix "-" -}}
{{- else -}}
{{- printf "%s-%s" .Release.Name $name | trunc 63 | trimSuffix "-" -}}
{{- end -}}
{{- end -}}
{{- end -}}

{{- define "forge.chart" -}}
{{- printf "%s-%s" .Chart.Name .Chart.Version | replace "+" "_" | trunc 63 | trimSuffix "-" -}}
{{- end -}}

{{- define "forge.labels" -}}
helm.sh/chart: {{ include "forge.chart" . }}
{{ include "forge.selectorLabels" . }}
app.kubernetes.io/version: {{ .Chart.AppVersion | quote }}
app.kubernetes.io/managed-by: {{ .Release.Service }}
app.kubernetes.io/part-of: forge
{{- end -}}

{{- define "forge.selectorLabels" -}}
app.kubernetes.io/name: {{ include "forge.name" . }}
app.kubernetes.io/instance: {{ .Release.Name }}
{{- end -}}

{{/*
The secret holding the session and hashing keys.

Refuses to render a Secret full of empty strings. The raw manifests shipped
literal CHANGE_ME placeholders, which forge-config reads as *present*: the pod
started and then failed deep inside the crypto or the database, instead of
failing immediately with a clear "secret not set".
*/}}
{{- define "forge.secretName" -}}
{{- if .Values.existingSecret -}}
{{- .Values.existingSecret -}}
{{- else -}}
{{- printf "%s-secrets" (include "forge.fullname" .) -}}
{{- end -}}
{{- end -}}

{{/*
The database URL, assembled from a value or from an existing secret.
*/}}
{{- define "forge.databaseUrl" -}}
{{- if .Values.database.url -}}
{{- .Values.database.url -}}
{{- end -}}
{{- end -}}

{{/*
Fully qualified image reference.

Fails the render rather than defaulting to `:latest`. `image: forge/server:latest`
in the previous manifests was not a pullable reference at all, so every deployment
attempt failed with ErrImagePull and the failure looked like a cluster problem
rather than a missing value.
*/}}
{{- define "forge.image" -}}
{{- $img := .Values.image -}}
{{- if $img.digest -}}
{{- printf "%s@%s" $img.repository $img.digest -}}
{{- else if $img.tag -}}
{{- printf "%s:%s" $img.repository $img.tag -}}
{{- else -}}
{{- fail "image.tag or image.digest is required. The previous manifests referenced forge/server:latest, which was never published; set image.repository and image.tag (or .digest) to a pushed image." -}}
{{- end -}}
{{- end -}}

{{- define "forge.consoleImage" -}}
{{- $img := .Values.console.image -}}
{{- if $img.digest -}}
{{- printf "%s@%s" $img.repository $img.digest -}}
{{- else if $img.tag -}}
{{- printf "%s:%s" $img.repository $img.tag -}}
{{- else -}}
{{- fail "console.image.tag or console.image.digest is required when console.enabled is true." -}}
{{- end -}}
{{- end -}}

{{/*
Environment shared by every Forge process.

FORGE_PUBLIC_BASE_URL is set explicitly because forge-config defaults it to
http://localhost:3000, which would otherwise end up advertised in the generated
OpenAPI document and in password-reset links.
*/}}
{{- define "forge.env" -}}
- name: FORGE_SERVER_HOST
  value: "0.0.0.0"
- name: FORGE_SERVER_PORT
  value: {{ .Values.service.port | quote }}
- name: FORGE_LOG_LEVEL
  value: {{ .Values.config.logLevel | quote }}
- name: FORGE_LOG_FORMAT
  value: {{ .Values.config.logFormat | quote }}
- name: FORGE_DEFAULT_TIMEZONE
  value: {{ .Values.config.defaultTimezone | quote }}
- name: FORGE_RATE_LIMIT_PER_SECOND
  value: {{ .Values.config.rateLimitPerSecond | quote }}
- name: FORGE_RATE_LIMIT_BURST
  value: {{ .Values.config.rateLimitBurst | quote }}
- name: FORGE_RETENTION_EXECUTIONS
  value: {{ printf "%dd" (int .Values.config.retention.executionsDays) | quote }}
- name: FORGE_RETENTION_AUDIT
  value: {{ printf "%dd" (int .Values.config.retention.auditDays) | quote }}
- name: FORGE_SCHEDULER_ENABLED
  value: {{ .Values.scheduler.enabled | quote }}
- name: FORGE_SCHEDULER_POLL_INTERVAL
  value: {{ .Values.scheduler.pollInterval | quote }}
- name: FORGE_SCHEDULER_BATCH_SIZE
  value: {{ .Values.scheduler.batchSize | quote }}
- name: FORGE_SCHEDULER_MAX_CATCH_UP
  value: {{ .Values.scheduler.maxCatchUp | quote }}
{{- if .Values.ingress.enabled }}
{{- $host := "" }}
{{- range .Values.ingress.hosts }}{{ $host = .host }}{{ end }}
- name: FORGE_PUBLIC_BASE_URL
  value: {{ printf "http://%s" $host | quote }}
{{- else }}
- name: FORGE_PUBLIC_BASE_URL
  value: {{ printf "http://%s:%d" (include "forge.fullname" .) (int .Values.service.port) | quote }}
{{- end }}
{{- range .Values.api.extraEnv }}
- {{ toYaml . | nindent 2 }}
{{- end }}
{{- end -}}
{{/*
Service account name, honouring an existing account when one is supplied.
*/}}
{{- define "forge.serviceAccountName" -}}
{{- if .Values.serviceAccount.create -}}
{{- default (printf "%s-sa" (include "forge.fullname" .)) .Values.serviceAccount.name -}}
{{- else -}}
{{- default "default" .Values.serviceAccount.name -}}
{{- end -}}
{{- end -}}
