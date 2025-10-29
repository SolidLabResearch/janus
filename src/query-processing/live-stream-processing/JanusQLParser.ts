/**
 * Interface defining the Historical as well as the Live Windows which will be 
 * Added in the JanusQL Query Language for Procesing Historical as well as Live Streaming Data
 */
export interface WindowDefinition {
  window_name: string;
  stream_name: string;
  width: number;
  slide: number;
  offset?: number;
  start?: number;
  end?: number;
  type: 'live' | 'historical-sliding' | 'historical-fixed';
}

/**
 * Interface for the R2S Operator for Result Stream. 
 */
export interface R2SOperator {
  operator: string;
  name: string;
}

/**
 * Interface for the Parsed Janus Query into live windows, historical windows
 * and SPARQL query for the historical data fetching and the RSP-QL query for the live data processing
 * with an RSP-QL engine like RSP-JS (which we use).
 */
export interface ParsedJanusQuery {
  r2s: R2SOperator | null;
  liveWindows: WindowDefinition[];
  historicalWindows: WindowDefinition[];
  rspqlQuery: string;
  sparqlQueries: string[];
  prefixes: Map<string, string>;
  whereClause: string;
  selectClause: string;
}

/**
 * Parser class for the JanusQL Query Language that can parse queries containing both live and historical window definitions.
 * It extracts live windows for RSP-QL processing and historical windows for SPARQL queries.
 */
export class JanusQLParser {
  parse(query: string): ParsedJanusQuery {
    const parsed: ParsedJanusQuery = {
      r2s: null,
      liveWindows: [],
      historicalWindows: [],
      rspqlQuery: '',
      sparqlQueries: [],
      prefixes: new Map<string, string>(),
      whereClause: '',
      selectClause: '',
    };

    const lines = query.split(/\r?\n/);
    const prefixLines: string[] = [];
    let inWhereClause = false;
    const whereLines: string[] = [];

    for (const line of lines) {
      const trimmed = line.trim();

      if (
        !trimmed ||
        trimmed.startsWith('/*') ||
        trimmed.startsWith('*') ||
        trimmed.startsWith('*/')
      ) {
        if (inWhereClause && trimmed) {
          whereLines.push(line);
        }
        continue;
      }

      if (trimmed.startsWith('REGISTER')) {
        const registerMatch = trimmed.match(/REGISTER\s+(\w+)\s+([^\s]+)\s+AS/);
        if (registerMatch && registerMatch[1] && registerMatch[2]) {
          parsed.r2s = {
            operator: registerMatch[1],
            name: this.unwrap(registerMatch[2], parsed.prefixes),
          };
        }
      }
      else if (trimmed.startsWith('SELECT')) {
        parsed.selectClause = trimmed;
      }
      else if (trimmed.startsWith('PREFIX')) {
        const prefixMatch = trimmed.match(/PREFIX\s+([^:]*?):\s*<([^>]+)>/);
        if (prefixMatch && prefixMatch[1] !== undefined && prefixMatch[2]) {
          parsed.prefixes.set(prefixMatch[1], prefixMatch[2]);
          prefixLines.push(trimmed);
        }
      }
      else if (trimmed.startsWith('FROM NAMED WINDOW')) {
        const window = this.parseWindow(trimmed, parsed.prefixes);
        if (window) {
          if (window.type === 'live') {
            parsed.liveWindows.push(window);
          } else {
            parsed.historicalWindows.push(window);
          }
        }
      }
      else if (trimmed.startsWith('WHERE')) {
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

  private parseWindow(line: string, prefixMapper: Map<string, string>): WindowDefinition | null {
    const historicalSlidingMatch = line.match(
      /FROM\s+NAMED\s+WINDOW\s+([^\s]+)\s+ON\s+STREAM\s+([^\s]+)\s+\[OFFSET\s+(\d+)\s+RANGE\s+(\d+)\s+STEP\s+(\d+)\]/
    );

    if (historicalSlidingMatch && historicalSlidingMatch[1] && historicalSlidingMatch[2]) {
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

    if (historicalFixedMatch && historicalFixedMatch[1] && historicalFixedMatch[2]) {
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

    if (liveSlidingMatch && liveSlidingMatch[1] && liveSlidingMatch[2]) {
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

  private generateRSPQLQuery(parsed: ParsedJanusQuery, prefixLines: string[]): string {
    const lines: string[] = [];

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

  private generateSPARQLQueries(parsed: ParsedJanusQuery, prefixLines: string[]): string[] {
    const queries: string[] = [];

    for (const window of parsed.historicalWindows) {
      const lines: string[] = [];

      prefixLines.forEach((prefix) => lines.push(prefix));
      lines.push('');

      if (parsed.selectClause) {
        lines.push(parsed.selectClause);
      }
      lines.push('');

      const wrappedWindowName = this.wrapIRI(window.window_name, parsed.prefixes);
      lines.push(`FROM NAMED ${wrappedWindowName}`);
      lines.push('');

      const whereClause = this.adaptWhereClauseForHistorical(
        parsed.whereClause,
        window,
        parsed.prefixes
      );
      lines.push(whereClause);

      queries.push(lines.join('\n'));
    }

    return queries;
  }

  private adaptWhereClauseForHistorical(
    whereClause: string,
    window: WindowDefinition,
    _prefixes: Map<string, string>
  ): string {

    let adapted = whereClause.replace(/WINDOW\s+/g, 'GRAPH ');

    if (
      window.type === 'historical-fixed' &&
      window.start !== undefined &&
      window.end !== undefined
    ) {

      const filterClause = `\n  FILTER(?timestamp >= ${window.start} && ?timestamp <= ${window.end})`;
      adapted = adapted.replace(/}(\s*)$/, `${filterClause}\n}$1`);
    } else if (window.type === 'historical-sliding' && window.offset !== undefined) {
      const filterClause = `\n  # Historical sliding window: offset=${window.offset}, range=${window.width}, step=${window.slide}`;
      adapted = adapted.replace(/}(\s*)$/, `${filterClause}\n}$1`);
    }

    return adapted;
  }

  private unwrap(prefixedIri: string, mapper: Map<string, string>): string {
    const trimmed = prefixedIri.trim();

    if (trimmed.startsWith('<') && trimmed.endsWith('>')) {
      return trimmed.slice(1, -1);
    }

    const colonIndex = trimmed.indexOf(':');
    if (colonIndex !== -1) {
      const prefix = trimmed.substring(0, colonIndex);
      const localPart = trimmed.substring(colonIndex + 1);

      if (mapper.has(prefix)) {
        return mapper.get(prefix)! + localPart;
      }
    }

    return trimmed;
  }

  private wrapIRI(iri: string, prefixes: Map<string, string>): string {
    for (const [prefix, namespace] of prefixes.entries()) {
      if (iri.startsWith(namespace)) {
        const localPart = iri.substring(namespace.length);
        return `${prefix}:${localPart}`;
      }
    }

    return `<${iri}>`;
  }
}
