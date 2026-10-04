# The Next.js console. Dependencies are installed in their own layer so a source
# change does not reinstall them.

FROM node:22-slim AS deps

WORKDIR /app
COPY package.json package-lock.json ./
# `npm ci` installs exactly the lockfile, which is reproducible.
RUN npm ci

FROM node:22-slim AS builder

WORKDIR /app
COPY --from=deps /app/node_modules ./node_modules
COPY . .

# `NEXT_PUBLIC_*` values are inlined at build time, so the API URL must be
# supplied here rather than at run time.
ARG NEXT_PUBLIC_API_URL=http://localhost:3000/api/v1
ENV NEXT_PUBLIC_API_URL=$NEXT_PUBLIC_API_URL
ENV NEXT_TELEMETRY_DISABLED=1

RUN npm run build

FROM node:22-slim AS runtime

WORKDIR /app
ENV NODE_ENV=production \
    NEXT_TELEMETRY_DISABLED=1

COPY --from=builder /app/.next ./.next
COPY --from=builder /app/public ./public
COPY --from=builder /app/node_modules ./node_modules
COPY --from=builder /app/package.json ./package.json
COPY --from=builder /app/next.config.ts ./next.config.ts

# Run unprivileged.
RUN useradd --create-home --uid 10001 nextjs && chown -R nextjs:nextjs /app
USER nextjs

EXPOSE 3000
CMD ["npm", "run", "start"]
