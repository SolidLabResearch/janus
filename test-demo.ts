/**
 * RDF Querying Demo
 * Demonstrates basic RDF data loading and SPARQL querying functionality
 */

import { InMemoryAdapter } from './src/adapters/InMemoryAdapter';
import { WasmAdapter } from './src/adapters/WasmAdapter';
import {
  RdfFormat,
  SelectQueryResult,
  AskQueryResult,
  ConstructQueryResult,
  RdfTermType,
} from './src/core/types';

async function main() {
  console.log('Janus RDF Querying Demo\n');

  // Test In-Memory Adapter
  console.log('Testing In-Memory Adapter\n');
  const inMemoryStore = new InMemoryAdapter();
  await testAdapter(inMemoryStore, 'In-Memory');

  // Test WASM Adapter
  console.log('\nTesting WASM Adapter\n');
  const wasmStore = await WasmAdapter.create();
  await testAdapter(wasmStore, 'WASM');
}

async function testAdapter(store: InMemoryAdapter | WasmAdapter, adapterName: string) {
  // Sample RDF data in N-Triples format
  const sampleData = `
<http://example.org/Alice> <http://xmlns.com/foaf/0.1/name> "Alice Johnson" .
<http://example.org/Alice> <http://xmlns.com/foaf/0.1/age> "30" .
<http://example.org/Alice> <http://xmlns.com/foaf/0.1/knows> <http://example.org/Bob> .
<http://example.org/Bob> <http://xmlns.com/foaf/0.1/name> "Bob Smith" .
<http://example.org/Bob> <http://xmlns.com/foaf/0.1/age> "25" .
<http://example.org/Charlie> <http://xmlns.com/foaf/0.1/name> "Charlie Brown" .
<http://example.org/Charlie> <http://xmlns.com/foaf/0.1/age> "35" .
  `.trim();

  console.log(`[${adapterName}] Loading RDF data...`);
  const loadedCount = await store.loadData(sampleData, RdfFormat.NTriples);
  console.log(`[${adapterName}] Loaded ${loadedCount} triples\n`);

  // Get store size
  const size = await store.size();
  console.log(`[${adapterName}] Store contains ${size} triples\n`);

  // Execute SELECT query
  console.log(`[${adapterName}] Executing SELECT query...`);
  const selectQuery = `
    SELECT ?subject ?predicate ?object ?id
    WHERE {
      ?subject ?predicate ?object .
    }
    LIMIT 5
  `;

  try {
    const selectResult = (await store.query(selectQuery)) as SelectQueryResult;
    console.log(`[${adapterName}] SELECT Results:`);
    console.log(`[${adapterName}] Variables: ${selectResult.head.vars.join(', ')}`);
    console.log(`[${adapterName}] Found ${selectResult.results.bindings.length} results\n`);

    selectResult.results.bindings.slice(0, 3).forEach((binding, index) => {
      console.log(`[${adapterName}] ${index + 1}. Subject: ${binding.subject?.value}`);
      console.log(`[${adapterName}]    Predicate: ${binding.predicate?.value}`);
      console.log(`[${adapterName}]    Object: ${binding.object?.value}\n`);
    });
  } catch (error) {
    console.log(
      `[${adapterName}] SELECT query failed:`,
      error instanceof Error ? error.message : String(error)
    );
  }

  // Execute ASK query
  console.log(`[${adapterName}] Executing ASK query...`);
  const askQuery = `
    ASK {
      ?person <http://xmlns.com/foaf/0.1/name> ?name .
      FILTER(CONTAINS(?name, "Alice"))
    }
  `;

  try {
    const askResult = (await store.query(askQuery)) as AskQueryResult;
    console.log(
      `[${adapterName}] Does anyone named Alice exist? ${askResult.boolean ? 'Yes' : 'No'}\n`
    );
  } catch (error) {
    console.log(
      `[${adapterName}] ASK query failed:`,
      error instanceof Error ? error.message : String(error)
    );
  }

  // Execute CONSTRUCT query
  console.log(`[${adapterName}] Executing CONSTRUCT query...`);
  const constructQuery = `
    CONSTRUCT {
      ?person <http://example.org/type> <http://xmlns.com/foaf/0.1/Person> .
      ?person <http://example.org/hasName> ?name .
    }
    WHERE {
      ?person <http://xmlns.com/foaf/0.1/name> ?name .
    }
  `;

  try {
    const constructResult = (await store.query(constructQuery)) as ConstructQueryResult;
    console.log(
      `[${adapterName}] CONSTRUCT Results: ${constructResult.triples.length} triples constructed\n`
    );

    constructResult.triples.slice(0, 2).forEach((triple, index) => {
      console.log(
        `[${adapterName}] ${index + 1}. ${triple.subject.value} → ${triple.predicate.value} → ${triple.object.value}`
      );
    });
    console.log();
  } catch (error) {
    console.log(
      `[${adapterName}] CONSTRUCT query failed:`,
      error instanceof Error ? error.message : String(error)
    );
  }

  // Insert a new triple
  console.log(`[${adapterName}] Inserting a new triple...`);
  await store.insert({
    subject: { type: RdfTermType.Uri, value: 'http://example.org/Alice' },
    predicate: { type: RdfTermType.Uri, value: 'http://xmlns.com/foaf/0.1/mbox' },
    object: { type: RdfTermType.Literal, value: 'alice@example.org' },
  });
  console.log(`[${adapterName}] Triple inserted\n`);

  // Check if triple exists
  console.log(`[${adapterName}] Checking if triple exists...`);
  const exists = await store.contains({
    subject: { type: RdfTermType.Uri, value: 'http://example.org/Alice' },
    predicate: { type: RdfTermType.Uri, value: 'http://xmlns.com/foaf/0.1/mbox' },
    object: { type: RdfTermType.Literal, value: 'alice@example.org' },
  });
  console.log(`[${adapterName}] Triple exists: ${exists}\n`);

  // Export data
  console.log(`[${adapterName}] Exporting data as Turtle...`);
  const exported = await store.export(RdfFormat.Turtle);
  console.log(`[${adapterName}] Exported data (first 200 chars):`);
  console.log(exported.substring(0, 200) + '...\n');

  // Clean up
  console.log(`[${adapterName}] Clearing store...`);
  await store.clear();
  const finalSize = await store.size();
  console.log(`[${adapterName}] Final store size: ${finalSize}\n`);
}

// Run the demo
if (require.main === module) {
  main().catch((error) => {
    console.error('Demo failed:', error);
    process.exit(1);
  });
}

export { main as runDemo };
