/**
 * Basic Usage Example for Janus RDF Template
 *
 * This example demonstrates the core functionality of the Janus RDF framework,
 * including working with different RDF stores via HTTP and WASM.
 */

import {
  OxigraphAdapter,
  JenaAdapter,
  RdfFormat,
  RdfTermType,
  SelectQueryResult,
  AskQueryResult,
} from '../src';

/**
 * Example 1: Working with Oxigraph HTTP Adapter
 */
async function oxigraphExample() {
  console.log('\n=== Oxigraph HTTP Adapter Example ===\n');

  // Create adapter for Oxigraph HTTP endpoint
  const adapter = new OxigraphAdapter({
    url: process.env.OXIGRAPH_ENDPOINT || 'http://localhost:7878',
    storeType: 'oxigraph',
    timeoutSecs: 30,
  });

  // Check if server is available
  const isAvailable = await adapter.ping();
  console.log('Oxigraph is available:', isAvailable);

  if (!isAvailable) {
    console.log('Oxigraph server is not running. Skipping example.');
    return;
  }

  // Sample RDF data in Turtle format
  const turtleData = `
    @prefix ex: <http://example.org/> .
    @prefix foaf: <http://xmlns.com/foaf/0.1/> .
    @prefix xsd: <http://www.w3.org/2001/XMLSchema#> .

    ex:Alice a foaf:Person ;
      foaf:name "Alice Johnson" ;
      foaf:age 30 ;
      foaf:knows ex:Bob, ex:Charlie .

    ex:Bob a foaf:Person ;
      foaf:name "Bob Smith" ;
      foaf:age 25 .

    ex:Charlie a foaf:Person ;
      foaf:name "Charlie Brown" ;
      foaf:age 35 ;
      foaf:knows ex:Alice .
  `;

  // Load data into the store
  console.log('Loading RDF data...');
  await adapter.loadData(turtleData, RdfFormat.Turtle);
  console.log('Data loaded successfully!');

  // Get store size
  const size = await adapter.size();
  console.log(`Store contains ${size} triples`);

  // Execute a SELECT query
  console.log('\nExecuting SELECT query...');
  const selectQuery = `
    PREFIX foaf: <http://xmlns.com/foaf/0.1/>
    SELECT ?name ?age WHERE {
      ?person a foaf:Person ;
        foaf:name ?name ;
        foaf:age ?age .
    }
    ORDER BY DESC(?age)
  `;
  const selectResult = (await adapter.query(selectQuery)) as SelectQueryResult;
  console.log('People in store:');
  selectResult.results.bindings.forEach((binding) => {
    console.log(`  - ${binding.name?.value} (age: ${binding.age?.value})`);
  });

  // Execute an ASK query
  console.log('\nExecuting ASK query...');
  const askQuery = `
    PREFIX foaf: <http://xmlns.com/foaf/0.1/>
    PREFIX ex: <http://example.org/>
    ASK {
      ex:Alice foaf:knows ex:Bob .
    }
  `;
  const askResult = (await adapter.query(askQuery)) as AskQueryResult;
  console.log('Does Alice know Bob?', askResult.boolean ? 'Yes' : 'No');

  // Insert a new triple
  console.log('\nInserting a new triple...');
  await adapter.insert({
    subject: { type: RdfTermType.Uri, value: 'http://example.org/Alice' },
    predicate: { type: RdfTermType.Uri, value: 'http://xmlns.com/foaf/0.1/mbox' },
    object: { type: RdfTermType.Literal, value: 'alice@example.org' },
  });
  console.log('Triple inserted!');

  // Check if triple exists
  const exists = await adapter.contains({
    subject: { type: RdfTermType.Uri, value: 'http://example.org/Alice' },
    predicate: { type: RdfTermType.Uri, value: 'http://xmlns.com/foaf/0.1/mbox' },
    object: { type: RdfTermType.Literal, value: 'alice@example.org' },
  });
  console.log('Triple exists in store:', exists);

  // Export data
  console.log('\nExporting data as TriG...');
  const exported = await adapter.export(RdfFormat.TriG);
  console.log('Exported data (first 500 chars):');
  console.log(exported.substring(0, 500) + '...');

  // Skipping cleanup to avoid update issues
  console.log('\nSkipping cleanup...');
}

/**
 * Example 2: Working with Apache Jena Fuseki Adapter
 */
async function jenaExample() {
  console.log('\n=== Apache Jena Fuseki Adapter Example ===\n');

  // Create adapter for Jena Fuseki
  const adapter = new JenaAdapter(
    {
      url: process.env.JENA_ENDPOINT || 'http://localhost:3030',
      storeType: 'jena',
      authToken: process.env.JENA_AUTH_TOKEN,
    },
    process.env.JENA_DATASET || 'ds'
  );

  // Check if server is available
  const isAvailable = await adapter.ping();
  console.log('Jena Fuseki is available:', isAvailable);

  if (!isAvailable) {
    console.log('Jena Fuseki server is not running. Skipping example.');
    return;
  }

  // Load data into default graph
  console.log('Loading RDF data into default graph...');
  const insertQuery = `
    PREFIX ex: <http://example.org/>
    PREFIX rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#>
    PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#>
    INSERT DATA {
      ex:Book1 rdf:type ex:Book ;
        ex:title "The Great Gatsby" ;
        ex:author ex:Fitzgerald ;
        ex:published "1925"^^<http://www.w3.org/2001/XMLSchema#gYear> .
      ex:Fitzgerald rdf:type ex:Author ;
        ex:name "F. Scott Fitzgerald" .
      ex:Book2 rdf:type ex:Book ;
        ex:title "To Kill a Mockingbird" ;
        ex:author ex:Lee ;
        ex:published "1960"^^<http://www.w3.org/2001/XMLSchema#gYear> .
      ex:Lee rdf:type ex:Author ;
        ex:name "Harper Lee" .
    }
  `;
  await adapter.executeUpdate(insertQuery);
  console.log('Data loaded successfully!');

  // List all graphs
  console.log('\nListing all graphs...');
  const graphs = await adapter.listGraphs();
  console.log('Available graphs:', graphs);

  // Query the default graph
  console.log('\nQuerying books from the default graph...');
  const query = `
    PREFIX ex: <http://example.org/>
    SELECT ?title ?authorName ?year WHERE {
      ?book a ex:Book ;
        ex:title ?title ;
        ex:author ?author ;
        ex:published ?year .
      ?author ex:name ?authorName .
    }
    ORDER BY ?year
  `;
  const result = (await adapter.query(query)) as SelectQueryResult;
  console.log('Books in store:');
  result.results.bindings.forEach((binding) => {
    console.log(
      `  - "${binding.title?.value}" by ${binding.authorName?.value} (${binding.year?.value})`
    );
  });

  // Get dataset statistics
  console.log('\nGetting dataset statistics...');
  const stats = await adapter.getStatistics();
  console.log(`Dataset contains ${stats.tripleCount} triples`);

  // Execute SPARQL UPDATE
  console.log('\nExecuting SPARQL UPDATE...');
  const updateQuery = `
    PREFIX ex: <http://example.org/>
    INSERT DATA {
      ex:Book3 a ex:Book ;
        ex:title "1984" ;
        ex:author ex:Orwell ;
        ex:published "1949"^^<http://www.w3.org/2001/XMLSchema#gYear> .
      ex:Orwell a ex:Author ;
        ex:name "George Orwell" .
    }
  `;
  await adapter.executeUpdate(updateQuery);
  console.log('New book added!');

  // Verify the update
  const updatedResult = (await adapter.query(query)) as SelectQueryResult;
  console.log('\nUpdated book list:');
  updatedResult.results.bindings.forEach((binding) => {
    console.log(
      `  - "${binding.title?.value}" by ${binding.authorName?.value} (${binding.year?.value})`
    );
  });

  // Export dataset data
  console.log('\nExporting dataset data...');
  const datasetData = await adapter.gspGet();
  console.log('Exported dataset data (first 500 chars):');
  console.log(datasetData.substring(0, 500) + '...');

  // Clean up
  console.log('\nCleaning up...');
  await adapter.clear();
  console.log('Store cleared!');
}

/**
 * Example 3: Advanced Query Patterns
 */
async function advancedQueryExample() {
  console.log('\n=== Advanced Query Patterns Example ===\n');

  const adapter = new OxigraphAdapter({
    url: process.env.OXIGRAPH_ENDPOINT || 'http://localhost:7878',
    storeType: 'oxigraph',
  });

  // Sample dataset with more complex relationships
  const complexData = `
    @prefix ex: <http://example.org/> .
    @prefix foaf: <http://xmlns.com/foaf/0.1/> .
    @prefix org: <http://www.w3.org/ns/org#> .
    @prefix skos: <http://www.w3.org/2004/02/skos/core#> .

    ex:Company1 a org:Organization ;
      skos:prefLabel "Tech Corp" ;
      org:hasMember ex:Alice, ex:Bob .

    ex:Alice a foaf:Person ;
      foaf:name "Alice Johnson" ;
      org:role ex:Manager ;
      foaf:knows ex:Charlie .

    ex:Bob a foaf:Person ;
      foaf:name "Bob Smith" ;
      org:role ex:Developer .

    ex:Charlie a foaf:Person ;
      foaf:name "Charlie Brown" ;
      org:role ex:Designer ;
      foaf:knows ex:Alice .
  `;

  await adapter.loadData(complexData, RdfFormat.Turtle);

  // OPTIONAL pattern
  console.log('Query with OPTIONAL pattern:');
  const optionalQuery = `
    PREFIX foaf: <http://xmlns.com/foaf/0.1/>
    PREFIX org: <http://www.w3.org/ns/org#>
    SELECT ?name ?role ?friend WHERE {
      ?person foaf:name ?name .
      OPTIONAL { ?person org:role ?role }
      OPTIONAL { ?person foaf:knows/foaf:name ?friend }
    }
  `;
  const optionalResult = (await adapter.query(optionalQuery)) as SelectQueryResult;
  optionalResult.results.bindings.forEach((binding) => {
    console.log(
      `  ${binding.name?.value} - Role: ${binding.role?.value || 'N/A'}, Knows: ${
        binding.friend?.value || 'N/A'
      }`
    );
  });

  // FILTER pattern
  console.log('\nQuery with FILTER:');
  const filterQuery = `
    PREFIX foaf: <http://xmlns.com/foaf/0.1/>
    SELECT ?name WHERE {
      ?person foaf:name ?name .
      FILTER(CONTAINS(LCASE(?name), "alice"))
    }
  `;
  const filterResult = (await adapter.query(filterQuery)) as SelectQueryResult;
  console.log('People with "alice" in name:');
  filterResult.results.bindings.forEach((binding) => {
    console.log(`  - ${binding.name?.value}`);
  });

  // Property path
  console.log('\nQuery with property path:');
  const pathQuery = `
    PREFIX foaf: <http://xmlns.com/foaf/0.1/>
    SELECT ?person1 ?person2 WHERE {
      ?person1 foaf:knows/foaf:knows ?person2 .
      FILTER(?person1 != ?person2)
    }
  `;
  const pathResult = (await adapter.query(pathQuery)) as SelectQueryResult;
  console.log('People connected through friends:');
  pathResult.results.bindings.forEach((binding) => {
    console.log(`  ${binding.person1?.value} -> ${binding.person2?.value}`);
  });

  await adapter.clear();
}

/**
 * Main execution
 */
async function main() {
  console.log('Janus RDF Template - Basic Usage Examples\n');
  console.log('='.repeat(50));

  // Parse command line arguments
  const args = process.argv.slice(2);
  let runOxigraph = args.includes('--oxigraph');
  let runJena = args.includes('--jena');
  let runAdvanced = args.includes('--advanced');

  // If no specific flags, run all
  if (!runOxigraph && !runJena && !runAdvanced) {
    runOxigraph = runJena = runAdvanced = true;
  }

  console.log(
    `Running examples: Oxigraph=${runOxigraph}, Jena=${runJena}, Advanced=${runAdvanced}\n`
  );

  try {
    // Run selected examples
    if (runOxigraph) await oxigraphExample();
    if (runJena) await jenaExample();
    if (runAdvanced) await advancedQueryExample();

    console.log('\n' + '='.repeat(50));
    console.log('\nSelected examples completed successfully!\n');
  } catch (error) {
    console.error('\nError running examples:');
    console.error(error);
    process.exit(1);
  }
}

// Run if executed directly
if (require.main === module) {
  main().catch((error) => {
    console.error('Fatal error:', error);
    process.exit(1);
  });
}

export { oxigraphExample, jenaExample, advancedQueryExample };
