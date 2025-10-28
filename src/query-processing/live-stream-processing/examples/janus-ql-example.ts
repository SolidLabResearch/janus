import { JanusQLParser } from '../JanusQLParser';

const exampleQuery = `
PREFIX :     <https://rsp.js/schema#>
REGISTER RStream :acclWindowing AS
SELECT *

FROM NAMED WINDOW :liveWindow ON STREAM :accelStream [RANGE 1000 STEP 10]
FROM NAMED WINDOW :historicalSliding ON STREAM :historicalAccl [OFFSET 2000 RANGE 1000 STEP 100]
FROM NAMED WINDOW :fixedHistorical ON STREAM :historicalAccl [START 1745857573 END 1738485859]
FROM NAMED WINDOW :tumblingLive ON STREAM :accelStream [RANGE 1000 STEP 1000]
FROM NAMED WINDOW :tumblingHistorical ON STREAM :accelStream [OFFSET 2000 RANGE 1000 STEP 1000]
WHERE {
 /*
 Space for selecting the values over the windows and computing.
 */
}
`;

function runExample() {
  const parser = new JanusQLParser();
  const result = parser.parse(exampleQuery);

  console.log('=== PARSED JANUS-QL QUERY ===\n');

  console.log('R2S Operator:', result.r2s);
  console.log('\nSelect Clause:', result.selectClause);

  console.log('\n--- Live Windows ---');
  result.liveWindows.forEach((window, idx) => {
    console.log(`\nLive Window ${idx + 1}:`);
    console.log(`  Name: ${window.window_name}`);
    console.log(`  Stream: ${window.stream_name}`);
    console.log(`  Range: ${window.width}ms`);
    console.log(`  Step: ${window.slide}ms`);
    console.log(`  Type: ${window.type}`);
  });

  console.log('\n--- Historical Windows ---');
  result.historicalWindows.forEach((window, idx) => {
    console.log(`\nHistorical Window ${idx + 1}:`);
    console.log(`  Name: ${window.window_name}`);
    console.log(`  Stream: ${window.stream_name}`);
    console.log(`  Type: ${window.type}`);

    if (window.type === 'historical-sliding') {
      console.log(`  Offset: ${window.offset}ms`);
      console.log(`  Range: ${window.width}ms`);
      console.log(`  Step: ${window.slide}ms`);
    } else if (window.type === 'historical-fixed') {
      console.log(`  Start: ${window.start}`);
      console.log(`  End: ${window.end}`);
    }
  });

  console.log('\n\n=== GENERATED RSP-QL QUERY (for Live Streaming) ===\n');
  console.log(result.rspqlQuery);

  console.log('\n\n=== GENERATED SPARQL QUERIES (for Historical Data) ===\n');
  result.sparqlQueries.forEach((query, idx) => {
    console.log(`\n--- SPARQL Query ${idx + 1} ---`);
    console.log(query);
  });

  console.log('\n\n=== PREFIXES ===');
  result.prefixes.forEach((value, key) => {
    console.log(`${key}: <${value}>`);
  });

  return result;
}

// Run the example if this file is executed directly
if (require.main === module) {
  runExample();
}

export { runExample, exampleQuery };
