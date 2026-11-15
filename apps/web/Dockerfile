FROM node:24-bookworm-slim AS dependencies

RUN corepack enable && corepack prepare pnpm@11.25.0 --activate
WORKDIR /app
COPY package.json pnpm-lock.yaml pnpm-workspace.yaml ./
COPY apps/web/package.json ./apps/web/package.json
RUN pnpm install --frozen-lockfile

FROM dependencies AS builder
ARG AXION_API_URL=http://axion:8080
ENV AXION_API_URL=${AXION_API_URL} NEXT_TELEMETRY_DISABLED=1
COPY apps/web ./apps/web
RUN pnpm --filter @axion/web build

FROM node:24-bookworm-slim AS runtime
WORKDIR /app
ENV NODE_ENV=production \
    NEXT_TELEMETRY_DISABLED=1 \
    HOSTNAME=0.0.0.0 \
    PORT=3000
RUN groupadd --gid 10001 axion && useradd --uid 10001 --gid axion --shell /usr/sbin/nologin axion
COPY --from=builder --chown=axion:axion /app/apps/web/.next/standalone ./
COPY --from=builder --chown=axion:axion /app/apps/web/.next/static ./apps/web/.next/static
COPY --from=builder --chown=axion:axion /app/apps/web/public ./apps/web/public
USER 10001:10001
EXPOSE 3000
HEALTHCHECK --interval=10s --timeout=3s --start-period=10s --retries=6 \
    CMD node -e "fetch('http://127.0.0.1:3000').then(r=>{if(!r.ok)process.exit(1)}).catch(()=>process.exit(1))"
CMD ["node", "apps/web/server.js"]

