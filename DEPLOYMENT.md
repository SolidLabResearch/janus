# Deployment Guide

This guide covers various deployment strategies for the Janus RDF Template project.

## Table of Contents

- [Prerequisites](#prerequisites)
- [Building for Production](#building-for-production)
- [Deployment Options](#deployment-options)
  - [NPM Package](#npm-package)
  - [Docker Container](#docker-container)
  - [Kubernetes](#kubernetes)
  - [Serverless (AWS Lambda)](#serverless-aws-lambda)
  - [Cloud Functions (Google Cloud)](#cloud-functions-google-cloud)
- [Configuration](#configuration)
- [Monitoring and Logging](#monitoring-and-logging)
- [Security Considerations](#security-considerations)
- [Performance Tuning](#performance-tuning)
- [Troubleshooting](#troubleshooting)

## Prerequisites

Before deploying, ensure you have:

- Node.js >= 18.0.0
- npm >= 9.0.0
- Rust >= 1.70.0 (for building from source)
- Access to target deployment platform
- Environment variables configured

## Building for Production

### 1. Clean Build

```bash
# Clean previous builds
npm run clean

# Install dependencies
npm ci --production=false

# Build TypeScript and Rust
npm run build
```

### 2. Optimize Build

```bash
# Set production environment
export NODE_ENV=production

# Build with optimizations
npm run build

# Optional: Build WASM with size optimizations
cd rust
wasm-pack build --target nodejs --release -- --features "optimization"
```

### 3. Verify Build

```bash
# Run tests
npm test

# Check bundle size
du -sh dist/ pkg/

# Verify WASM module
node -e "console.log(require('./pkg'))"
```

## Deployment Options

### NPM Package

Deploy as an npm package for use in other projects.

#### 1. Prepare Package

```bash
# Update version
npm version patch  # or minor, major

# Build for production
npm run build

# Test package locally
npm pack
npm install -g ./janus-rdf-template-*.tgz
```

#### 2. Publish to NPM

```bash
# Login to npm
npm login

# Publish package
npm publish

# Publish with tag
npm publish --tag beta
```

#### 3. Publish to Private Registry

```bash
# Configure registry
npm config set registry https://your-registry.com

# Publish
npm publish --registry https://your-registry.com
```

### Docker Container

Containerize the application for consistent deployment.

#### 1. Create Dockerfile

```dockerfile
# Multi-stage build
FROM rust:1.75 as rust-builder

WORKDIR /build
COPY rust/ ./rust/
WORKDIR /build/rust
RUN cargo build --release

FROM node:18-alpine as node-builder

WORKDIR /build
RUN apk add --no-cache curl gcc g++ make python3

# Install wasm-pack
RUN curl https://rustwasm.github.io/wasm-pack/installer/init.sh -sSf | sh

COPY package*.json ./
RUN npm ci

COPY . .
COPY --from=rust-builder /build/rust/target/release/ ./rust/target/release/

RUN npm run build

# Production stage
FROM node:18-alpine

WORKDIR /app

# Install production dependencies only
COPY package*.json ./
RUN npm ci --production

# Copy built artifacts
COPY --from=node-builder /build/dist ./dist
COPY --from=node-builder /build/pkg ./pkg

# Set environment
ENV NODE_ENV=production
ENV PORT=3000

# Create non-root user
RUN addgroup -g 1001 -S nodejs && \
    adduser -S nodejs -u 1001

USER nodejs

EXPOSE 3000

CMD ["node", "dist/index.js"]
```

#### 2. Build and Run

```bash
# Build image
docker build -t janus-rdf:latest .

# Run container
docker run -d \
  -p 3000:3000 \
  -e OXIGRAPH_ENDPOINT=http://oxigraph:7878 \
  -e LOG_LEVEL=info \
  --name janus-app \
  janus-rdf:latest

# View logs
docker logs -f janus-app
```

#### 3. Docker Compose

```yaml
version: '3.8'

services:
  janus:
    build: .
    ports:
      - "3000:3000"
    environment:
      - NODE_ENV=production
      - OXIGRAPH_ENDPOINT=http://oxigraph:7878
      - JENA_ENDPOINT=http://fuseki:3030
      - LOG_LEVEL=info
    depends_on:
      - oxigraph
      - fuseki
    restart: unless-stopped

  oxigraph:
    image: oxigraph/oxigraph:latest
    ports:
      - "7878:7878"
    volumes:
      - oxigraph-data:/data
    command: serve --location /data --bind 0.0.0.0:7878

  fuseki:
    image: stain/jena-fuseki:latest
    ports:
      - "3030:3030"
    environment:
      - ADMIN_PASSWORD=admin
      - JVM_ARGS=-Xmx2g
    volumes:
      - fuseki-data:/fuseki

volumes:
  oxigraph-data:
  fuseki-data:
```

Run with:

```bash
docker-compose up -d
```

### Kubernetes

Deploy to a Kubernetes cluster.

#### 1. Create Kubernetes Manifests

deployment.yaml

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: janus-rdf
  labels:
    app: janus-rdf
spec:
  replicas: 3
  selector:
    matchLabels:
      app: janus-rdf
  template:
    metadata:
      labels:
        app: janus-rdf
    spec:
      containers:
      - name: janus
        image: your-registry/janus-rdf:latest
        ports:
        - containerPort: 3000
        env:
        - name: NODE_ENV
          value: "production"
        - name: OXIGRAPH_ENDPOINT
          valueFrom:
            configMapKeyRef:
              name: janus-config
              key: oxigraph-endpoint
        - name: LOG_LEVEL
          value: "info"
        resources:
          requests:
            memory: "256Mi"
            cpu: "250m"
          limits:
            memory: "512Mi"
            cpu: "500m"
        livenessProbe:
          httpGet:
            path: /health
            port: 3000
          initialDelaySeconds: 30
          periodSeconds: 10
        readinessProbe:
          httpGet:
            path: /ready
            port: 3000
          initialDelaySeconds: 5
          periodSeconds: 5
```

service.yaml

```yaml
apiVersion: v1
kind: Service
metadata:
  name: janus-rdf-service
spec:
  selector:
    app: janus-rdf
  ports:
  - protocol: TCP
    port: 80
    targetPort: 3000
  type: LoadBalancer
```

configmap.yaml

```yaml
apiVersion: v1
kind: ConfigMap
metadata:
  name: janus-config
data:
  oxigraph-endpoint: "http://oxigraph-service:7878"
  jena-endpoint: "http://fuseki-service:3030"
```

#### 2. Deploy

```bash
# Apply configurations
kubectl apply -f configmap.yaml
kubectl apply -f deployment.yaml
kubectl apply -f service.yaml

# Check status
kubectl get pods -l app=janus-rdf
kubectl get services

# View logs
kubectl logs -l app=janus-rdf -f

# Scale deployment
kubectl scale deployment janus-rdf --replicas=5
```

### Serverless (AWS Lambda)

Deploy as AWS Lambda function.

#### 1. Create Lambda Handler

lambda/index.js

```javascript
const { OxigraphAdapter } = require('janus-rdf-template');

exports.handler = async (event) => {
  try {
    const adapter = new OxigraphAdapter({
      url: process.env.OXIGRAPH_ENDPOINT,
      storeType: 'oxigraph',
    });

    const query = event.query || 'SELECT * WHERE { ?s ?p ?o } LIMIT 10';
    const result = await adapter.query(query);

    return {
      statusCode: 200,
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(result),
    };
  } catch (error) {
    return {
      statusCode: 500,
      body: JSON.stringify({ error: error.message }),
    };
  }
};
```

#### 2. Package for Lambda

```bash
# Install production dependencies
npm ci --production

# Create deployment package
zip -r lambda.zip . -x "*.git*" "test/*" "docs/*"
```

#### 3. Deploy with AWS CLI

```bash
# Create Lambda function
aws lambda create-function \
  --function-name janus-rdf-query \
  --runtime nodejs18.x \
  --role arn:aws:iam::ACCOUNT_ID:role/lambda-role \
  --handler lambda/index.handler \
  --zip-file fileb://lambda.zip \
  --timeout 30 \
  --memory-size 512 \
  --environment Variables="{OXIGRAPH_ENDPOINT=http://endpoint:7878}"

# Update function
aws lambda update-function-code \
  --function-name janus-rdf-query \
  --zip-file fileb://lambda.zip
```

#### 4. Deploy with Serverless Framework

serverless.yml

```yaml
service: janus-rdf

provider:
  name: aws
  runtime: nodejs18.x
  region: us-east-1
  memorySize: 512
  timeout: 30
  environment:
    OXIGRAPH_ENDPOINT: ${env:OXIGRAPH_ENDPOINT}
    NODE_ENV: production

functions:
  query:
    handler: lambda/index.handler
    events:
      - http:
          path: query
          method: post
          cors: true

plugins:
  - serverless-offline

package:
  exclude:
    - test/**
    - docs/**
    - .git/**
```

Deploy:

```bash
serverless deploy
```

### Cloud Functions (Google Cloud)

Deploy to Google Cloud Functions.

#### 1. Create Function

index.js

```javascript
const { OxigraphAdapter } = require('janus-rdf-template');

exports.queryRdf = async (req, res) => {
  try {
    const adapter = new OxigraphAdapter({
      url: process.env.OXIGRAPH_ENDPOINT,
      storeType: 'oxigraph',
    });

    const query = req.body.query || 'SELECT * WHERE { ?s ?p ?o } LIMIT 10';
    const result = await adapter.query(query);

    res.status(200).json(result);
  } catch (error) {
    res.status(500).json({ error: error.message });
  }
};
```

#### 2. Deploy

```bash
gcloud functions deploy queryRdf \
  --runtime nodejs18 \
  --trigger-http \
  --allow-unauthenticated \
  --entry-point queryRdf \
  --memory 512MB \
  --timeout 60s \
  --set-env-vars OXIGRAPH_ENDPOINT=http://endpoint:7878
```

## Configuration

### Environment Variables

Create a `.env.production` file:

```env
# Node Environment
NODE_ENV=production
PORT=3000

# RDF Store Endpoints
OXIGRAPH_ENDPOINT=http://oxigraph:7878
JENA_ENDPOINT=http://fuseki:3030
JENA_DATASET=production

# Authentication
JENA_AUTH_TOKEN=your-secure-token
API_KEY=your-api-key

# Logging
LOG_LEVEL=info
LOG_FORMAT=json

# Performance
ENABLE_WASM=true
CACHE_ENABLED=true
CACHE_TTL=3600

# Security
CORS_ORIGIN=https://yourdomain.com
RATE_LIMIT=100
```

### Configuration Management

Use tools like:

- dotenv for local development
- AWS Systems Manager Parameter Store for AWS
- Google Secret Manager for GCP
- Kubernetes Secrets for K8s

## Monitoring and Logging

### Application Monitoring

#### 1. Health Check Endpoint

```typescript
app.get('/health', (req, res) => {
  res.status(200).json({
    status: 'healthy',
    uptime: process.uptime(),
    timestamp: Date.now(),
  });
});
```

#### 2. Prometheus Metrics

```typescript
import * as promClient from 'prom-client';

const register = new promClient.Registry();

const queryCounter = new promClient.Counter({
  name: 'rdf_queries_total',
  help: 'Total number of RDF queries',
  labelNames: ['status'],
});

register.registerMetric(queryCounter);

app.get('/metrics', async (req, res) => {
  res.set('Content-Type', register.contentType);
  res.end(await register.metrics());
});
```

### Logging

#### Structured Logging

```typescript
import winston from 'winston';

const logger = winston.createLogger({
  level: process.env.LOG_LEVEL || 'info',
  format: winston.format.json(),
  transports: [
    new winston.transports.Console(),
    new winston.transports.File({ filename: 'error.log', level: 'error' }),
    new winston.transports.File({ filename: 'combined.log' }),
  ],
});
```

#### Log Aggregation

- ELK Stack (Elasticsearch, Logstash, Kibana)
- CloudWatch Logs (AWS)
- Google Cloud Logging (GCP)
- Datadog

## Security Considerations

### 1. Authentication

```typescript
import jwt from 'jsonwebtoken';

const authMiddleware = (req, res, next) => {
  const token = req.headers.authorization?.split(' ')[1];
  
  if (!token) {
    return res.status(401).json({ error: 'No token provided' });
  }
  
  try {
    const decoded = jwt.verify(token, process.env.JWT_SECRET);
    req.user = decoded;
    next();
  } catch (error) {
    res.status(401).json({ error: 'Invalid token' });
  }
};
```

### 2. Rate Limiting

```typescript
import rateLimit from 'express-rate-limit';

const limiter = rateLimit({
  windowMs: 15 * 60 * 1000, // 15 minutes
  max: 100, // limit each IP to 100 requests per windowMs
});

app.use('/api/', limiter);
```

### 3. Input Validation

```typescript
import { validateSparqlQuery } from './utils/validators';

app.post('/query', (req, res) => {
  const { query } = req.body;
  
  if (!validateSparqlQuery(query)) {
    return res.status(400).json({ error: 'Invalid SPARQL query' });
  }
  
  // Process query...
});
```

### 4. HTTPS/TLS

Always use HTTPS in production:

```bash
# Generate SSL certificate
certbot certonly --standalone -d yourdomain.com

# Use in Node.js
const https = require('https');
const fs = require('fs');

const options = {
  key: fs.readFileSync('/etc/letsencrypt/live/yourdomain.com/privkey.pem'),
  cert: fs.readFileSync('/etc/letsencrypt/live/yourdomain.com/fullchain.pem'),
};

https.createServer(options, app).listen(443);
```

## Performance Tuning

### 1. Connection Pooling

```typescript
const adapter = new OxigraphAdapter({
  url: endpoint,
  storeType: 'oxigraph',
  // Configure connection pool
  maxConnections: 50,
  keepAlive: true,
});
```

### 2. Caching

```typescript
import NodeCache from 'node-cache';

const cache = new NodeCache({ stdTTL: 600 }); // 10 minutes

async function cachedQuery(query: string) {
  const cacheKey = `query:${query}`;
  const cached = cache.get(cacheKey);
  
  if (cached) {
    return cached;
  }
  
  const result = await adapter.query(query);
  cache.set(cacheKey, result);
  return result;
}
```

### 3. Load Balancing

Use nginx as a reverse proxy:

```nginx
upstream janus_backend {
    least_conn;
    server localhost:3000;
    server localhost:3001;
    server localhost:3002;
}

server {
    listen 80;
    server_name yourdomain.com;

    location / {
        proxy_pass http://janus_backend;
        proxy_http_version 1.1;
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection 'upgrade';
        proxy_set_header Host $host;
        proxy_cache_bypass $http_upgrade;
    }
}
```

## Troubleshooting

### Common Issues

#### 1. WASM Module Not Loading

```bash
# Rebuild WASM
npm run build:rust

# Check WASM file exists
ls -la pkg/*.wasm

# Test loading
node -e "console.log(require('./pkg'))"
```

#### 2. Memory Issues

```bash
# Increase Node.js memory
NODE_OPTIONS="--max-old-space-size=4096" npm start

# Monitor memory
node --inspect index.js
```

#### 3. Connection Timeouts

```typescript
// Increase timeout
const adapter = new OxigraphAdapter({
  url: endpoint,
  storeType: 'oxigraph',
  timeoutSecs: 60, // Increase to 60 seconds
});
```

#### 4. Build Failures

```bash
# Clean and rebuild
npm run clean
rm -rf node_modules package-lock.json
npm install
npm run build
```

### Debug Mode

Enable debug logging:

```bash
export DEBUG=janus:*
export LOG_LEVEL=debug
npm start
```

### Performance Profiling

```bash
# Profile Node.js
node --prof index.js

# Generate report
node --prof-process isolate-*.log > profile.txt

# Flame graph
npm install -g clinic
clinic flame -- node index.js
```

## Backup and Recovery

### Database Backup

```bash
# Backup Oxigraph
curl http://localhost:7878/store > backup.nq

# Backup Jena Fuseki
curl -u admin:password http://localhost:3030/dataset/data > backup.ttl
```

### Disaster Recovery

1. Maintain regular backups
2. Test restore procedures
3. Use multi-region deployment
4. Implement failover mechanisms

## Support

For deployment issues:
- Check logs first
- Review [GitHub Issues](https://github.com/yourusername/janus/issues)
- Contact support@example.com
- Join our [Discord community](https://discord.gg/example)

---

Last Updated: 2024
Version: 0.1.0