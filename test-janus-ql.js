// Simple test script for JanusQL Parser
// Run with: node test-janus-ql.js

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
 WINDOW :liveWindow {
   ?s ?p ?o .
   ?s :timestamp ?timestamp .
   ?s :acceleration ?accel .
 }
 WINDOW :historicalSliding {
   ?hs ?hp ?ho .
   ?hs :timestamp ?hTimestamp .
 }
}
`;

// Mock implementation for testing (without TypeScript compilation)
class JanusQLParser {
  parse(query) {
    const parsed = {
      r2s: null,
      liveWindows: [],
      historicalWindows: [],
      rspqlQuery: '',
      sparqlQueries: [],
      prefixes: new Map(),
      whereClause: '',
      selectClause: '',
    };

    const lines = query.split(/\r?\n/);
    const prefixLines = [];
    let inWhereClause = false;
    const whereLines = [];

    for (const line of lines) {
      const trimmed = line.trim();

      if (!trimmed || trimmed.startsWith('/*') || trimmed.startsWith('*') || trimmed.startsWith('*/')) {
        if (inWhereClause && trimmed) {
          whereLines.push(line);
        }
        continue;
      }

      if (trimmed.startsWith('REGISTER')) {
        const registerMatch = trimmed.match(/REGISTER\s+(\w+)\s+([^\s]+)\s+AS/);
        if (registerMatch) {
          parsed.r2s = {
            operator: registerMatch[1],
            name: this.unwrap(registerMatch[2], parsed.prefixes),
          };
        }
      } else if (trimmed.startsWith('SELECT')) {
        parsed.selectClause = trimmed;
      } else if (trimmed.startsWith('PREFIX')) {
        const prefixMatch = trimmed.match(/PREFIX\s+([^:]*?):\s*<([^>]+)>/);
        if (prefixMatch) {
          parsed.prefixes.set(prefixMatch[1], prefixMatch[2]);
          prefixLines.push(trimmed);
        }
      } else if (trimmed.startsWith('FROM NAMED WINDOW')) {
        const window = this.parseWindow(trimmed, parsed.prefixes);
        if (window) {
          if (window.type === 'live') {
            parsed.liveWindows.push(window);
          } else {
            parsed.historicalWindows.push(window);
          }
        }
      } else if (trimmed.startsWith('WHERE')) {
        inWhereClause = true;
        whereLines.push(line);
      } else if (inWhereClause) {
        whereLines.push(line);
      }
    }

    parsed.whereClause = whereLines.join('\n');
    if (parsed.liveWindows.length > 0) {
      parsed.rspqlQuery = this.generateRSPQLQuery(parsed, prefixLines);
    }
    parsed.sparqlQueries = this.generateSPARQLQueries(parsed, prefixLines);

    return parsed;
  }

  parseWindow(line, prefixMapper) {
    const historicalSlidingMatch = line.match(
      /FROM\s+NAMED\s+WINDOW\s+([^\s]+)\s+ON\s+STREAM\s+([^\s]+)\s+\[OFFSET\s+(\d+)\s+RANGE\s+(\d+)\s+STEP\s+(\d+)\]/
    );

    if (historicalSlidingMatch) {
      return {
        window_name: this.unwrap(historicalSlidingMatch[1], prefixMapper),
        stream_name: this.unwrap(historicalSlidingMatch[2], prefixMapper),
        offset: Number(historicalSlidingMatch[3]),
        width: Number(historicalSlidingMatch[4]),
        slide: Number(historicalSlidingMatch[5]),
        type: 'historical-sliding',
      };
    }

    const historicalFixedMatch = line.match(
      /FROM\s+NAMED\s+WINDOW\s+([^\s]+)\s+ON\s+STREAM\s+([^\s]+)\s+\[START\s+(\d+)\s+END\s+(\d+)\]/
    );

    if (historicalFixedMatch) {
      return {
        window_name: this.unwrap(historicalFixedMatch[1], prefixMapper),
        stream_name: this.unwrap(historicalFixedMatch[2], prefixMapper),
        start: Number(historicalFixedMatch[3]),
        end: Number(historicalFixedMatch[4]),
        width: 0,
        slide: 0,
        type: 'historical-fixed',
      };
    }

    const liveSlidingMatch = line.match(
      /FROM\s+NAMED\s+WINDOW\s+([^\s]+)\s+ON\s+STREAM\s+([^\s]+)\s+\[RANGE\s+(\d+)\s+STEP\s+(\d+)\]/
    );

    if (liveSlidingMatch) {
      return {
        window_name: this.unwrap(liveSlidingMatch[1], prefixMapper),
        stream_name: this.unwrap(liveSlidingMatch[2], prefixMapper),
        width: Number(liveSlidingMatch[3]),
        slide: Number(liveSlidingMatch[4]),
        type: 'live',
      };
    }

    return null;
  }

  generateRSPQLQuery(parsed, prefixLines) {
    const lines = [];
    prefixLines.forEach((prefix) => lines.push(prefix));
    lines.push('');

    if (parsed.r2s) {
      const wrappedName = this.wrapIRI(parsed.r2s.name, parsed.prefixes);
      lines.push(`REGISTER ${parsed.r2s.operator} ${wrappedName} AS`);
    }

    if (parsed.selectClause) {
      lines.push(parsed.selectClause);
    }
    lines.push('');

    parsed.liveWindows.forEach((window) => {
      const wrappedWindowName = this.wrapIRI(window.window_name, parsed.prefixes);
      const wrappedStreamName = this.wrapIRI(window.stream_name, parsed.prefixes);
      lines.push(
        `FROM NAMED WINDOW ${wrappedWindowName} ON STREAM ${wrappedStreamName} [RANGE ${window.width} STEP ${window.slide}]`
      );
    });

    if (parsed.whereClause) {
      lines.push(parsed.whereClause);
    }

    return lines.join('\n');
  }

  generateSPARQLQueries(parsed, prefixLines) {
    const queries = [];

    for (const window of parsed.historicalWindows) {
      const lines = [];
      prefixLines.forEach((prefix) => lines.push(prefix));
      lines.push('');

      if (parsed.selectClause) {
        lines.push(parsed.selectClause);
      }
      lines.push('');

      const wrappedWindowName = this.wrapIRI(window.window_name, parsed.prefixes);
      lines.push(`FROM NAMED ${wrappedWindowName}`);
      lines.push('');

      const whereClause = this.adaptWhereClauseForHistorical(parsed.whereClause, window);
      lines.push(whereClause);

      queries.push(lines.join('\n'));
    }

    return queries;
  }

  adaptWhereClauseForHistorical(whereClause, window) {
    let adapted = whereClause.replace(/WINDOW\s+/g, 'GRAPH ');

    if (window.type === 'historical-fixed' && window.start !== undefined && window.end !== undefined) {
      const filterClause = `\n  FILTER(?timestamp >= ${window.start} && ?timestamp <= ${window.end})`;
      adapted = adapted.replace(/}(\s*)$/, `${filterClause}\n}$1`);
    } else if (window.type === 'historical-sliding' && window.offset !== undefined) {
      const filterClause = `\n  # Historical sliding window: offset=${window.offset}, range=${window.width}, step=${window.slide}`;
      adapted = adapted.replace(/}(\s*)$/, `${filterClause}\n}$1`);
    }

    return adapted;
  }

  unwrap(prefixedIri, mapper) {
    const trimmed = prefixedIri.trim();

    if (trimmed.startsWith('<') && trimmed.endsWith('>')) {
      return trimmed.slice(1, -1);
    }

    const colonIndex = trimmed.indexOf(':');
    if (colonIndex !== -1) {
      const prefix = trimmed.substring(0, colonIndex);
      const localPart = trimmed.substring(colonIndex + 1);

      if (mapper.has(prefix)) {
        return mapper.get(prefix) + localPart;
      }
    }

    return trimmed;
  }

  wrapIRI(iri, prefixes) {
    for (const [prefix, namespace] of prefixes.entries()) {
      if (iri.startsWith(namespace)) {
        const localPart = iri.substring(namespace.length);
        return `${prefix}:${localPart}`;
      }
    }

    return `<${iri}>`;
  }
}

// Run the test
console.log('='.repeat(80));
console.log('JANUS-QL PARSER TEST');
console.log('='.repeat(80));
console.log('\n--- INPUT QUERY ---\n');
console.log(exampleQuery);

const parser = new JanusQLParser();
const result = parser.parse(exampleQuery);

console.log('\n' + '='.repeat(80));
console.log('PARSED RESULTS');
console.log('='.repeat(80));

console.log('\n--- R2S Operator ---');
console.log(JSON.stringify(result.r2s, null, 2));

console.log('\n--- Select Clause ---');
console.log(result.selectClause);

console.log('\n--- Prefixes ---');
console.log('Prefix mappings:');
for (const [key, value] of result.prefixes.entries()) {
  console.log(`  ${key}: <${value}>`);
}

console.log('\n--- Live Windows ---');
console.log(`Found ${result.liveWindows.length} live window(s):`);
result.liveWindows.forEach((window, idx) => {
  console.log(`\n  ${idx + 1}. ${window.window_name}`);
  console.log(`     Stream: ${window.stream_name}`);
  console.log(`     Type: ${window.type}`);
  console.log(`     Range: ${window.width}ms, Step: ${window.slide}ms`);
});

console.log('\n--- Historical Windows ---');
console.log(`Found ${result.historicalWindows.length} historical window(s):`);
result.historicalWindows.forEach((window, idx) => {
  console.log(`\n  ${idx + 1}. ${window.window_name}`);
  console.log(`     Stream: ${window.stream_name}`);
  console.log(`     Type: ${window.type}`);

  if (window.type === 'historical-sliding') {
    console.log(`     Offset: ${window.offset}ms`);
    console.log(`     Range: ${window.width}ms, Step: ${window.slide}ms`);
  } else if (window.type === 'historical-fixed') {
    console.log(`     Start: ${window.start} (${new Date(window.start * 1000).toISOString()})`);
    console.log(`     End: ${window.end} (${new Date(window.end * 1000).toISOString()})`);
  }
});

console.log('\n' + '='.repeat(80));
console.log('GENERATED RSP-QL QUERY (Live Streaming)');
console.log('='.repeat(80));
console.log('\n' + result.rspqlQuery);

console.log('\n' + '='.repeat(80));
console.log('GENERATED SPARQL QUERIES (Historical Data)');
console.log('='.repeat(80));

result.sparqlQueries.forEach((query, idx) => {
  console.log(`\n--- SPARQL Query ${idx + 1} for: ${result.historicalWindows[idx].window_name} ---\n`);
  console.log(query);
});

console.log('\n' + '='.repeat(80));
console.log('TEST COMPLETE');
console.log('='.repeat(80));
