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

export interface R2SOperator {
  operator: string;
  name: string;
}

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

      // Skip empty lines and comments
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

      // Parse REGISTER statement
      if (trimmed.startsWith('REGISTER')) {
        const registerMatch = trimmed.match(/REGISTER\s+(\w+)\s+([^\s]+)\s+AS/);
        if (registerMatch && registerMatch[1] && registerMatch[2]) {
          parsed.r2s = {
            operator: registerMatch[1],
            name: this.unwrap(registerMatch[2], parsed.prefixes),
          };
        }
      }
      // Parse SELECT statement
      else if (trimmed.startsWith('SELECT')) {
        parsed.selectClause = trimmed;
      }
      // Parse PREFIX statement
      else if (trimmed.startsWith('PREFIX')) {
        const prefixMatch = trimmed.match(/PREFIX\s+([^:]*?):\s*<([^>]+)>/);
        if (prefixMatch && prefixMatch[1] !== undefined && prefixMatch[2]) {
          parsed.prefixes.set(prefixMatch[1], prefixMatch[2]);
          prefixLines.push(trimmed);
        }
      }
      // Parse FROM NAMED WINDOW statements
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
      // Parse WHERE clause
      else if (trimmed.startsWith('WHERE')) {
        inWhereClause = true;
        whereLines.push(line);
      } else if (inWhereClause) {
        whereLines.push(line);
      }
    }

    parsed.whereClause = whereLines.join('\n');

    // Generate RSP-QL query for live windows
    if (parsed.liveWindows.length > 0) {
      parsed.rspqlQuery = this.generateRSPQLQuery(parsed, prefixLines);
    }

    // Generate SPARQL queries for historical windows
    parsed.sparqlQueries = this.generateSPARQLQueries(parsed, prefixLines);

    return parsed;
  }

  private parseWindow(line: string, prefixMapper: Map<string, string>): WindowDefinition | null {
    // Pattern for sliding window with OFFSET (historical sliding)
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

    // Pattern for fixed historical window with START and END
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

    // Pattern for live sliding window (no OFFSET, START, or END)
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

    // Add prefixes
    prefixLines.forEach((prefix) => lines.push(prefix));
    lines.push('');

    // Add REGISTER statement
    if (parsed.r2s) {
      const wrappedName = this.wrapIRI(parsed.r2s.name, parsed.prefixes);
      lines.push(`REGISTER ${parsed.r2s.operator} ${wrappedName} AS`);
    }

    // Add SELECT clause
    if (parsed.selectClause) {
      lines.push(parsed.selectClause);
    }
    lines.push('');

    // Add only live windows
    parsed.liveWindows.forEach((window) => {
      const wrappedWindowName = this.wrapIRI(window.window_name, parsed.prefixes);
      const wrappedStreamName = this.wrapIRI(window.stream_name, parsed.prefixes);
      lines.push(
        `FROM NAMED WINDOW ${wrappedWindowName} ON STREAM ${wrappedStreamName} [RANGE ${window.width} STEP ${window.slide}]`
      );
    });

    // Add WHERE clause
    if (parsed.whereClause) {
      lines.push(parsed.whereClause);
    }

    return lines.join('\n');
  }

  private generateSPARQLQueries(parsed: ParsedJanusQuery, prefixLines: string[]): string[] {
    const queries: string[] = [];

    for (const window of parsed.historicalWindows) {
      const lines: string[] = [];

      // Add prefixes
      prefixLines.forEach((prefix) => lines.push(prefix));
      lines.push('');

      // Add SELECT clause
      if (parsed.selectClause) {
        lines.push(parsed.selectClause);
      }
      lines.push('');

      // Add FROM NAMED for this specific historical window
      const wrappedWindowName = this.wrapIRI(window.window_name, parsed.prefixes);
      lines.push(`FROM NAMED ${wrappedWindowName}`);
      lines.push('');

      // Add WHERE clause with timestamp filters
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
    // Replace WINDOW references with GRAPH references
    let adapted = whereClause.replace(/WINDOW\s+/g, 'GRAPH ');

    // Add timestamp filters based on window type
    if (
      window.type === 'historical-fixed' &&
      window.start !== undefined &&
      window.end !== undefined
    ) {
      // For fixed historical windows, add FILTER for timestamp range
      const filterClause = `\n  FILTER(?timestamp >= ${window.start} && ?timestamp <= ${window.end})`;
      adapted = adapted.replace(/}(\s*)$/, `${filterClause}\n}$1`);
    } else if (window.type === 'historical-sliding' && window.offset !== undefined) {
      // For sliding historical windows, add comment with window parameters
      // Future enhancement: compute actual timestamp filters
      const filterClause = `\n  # Historical sliding window: offset=${window.offset}, range=${window.width}, step=${window.slide}`;
      adapted = adapted.replace(/}(\s*)$/, `${filterClause}\n}$1`);
    }

    return adapted;
  }

  private unwrap(prefixedIri: string, mapper: Map<string, string>): string {
    const trimmed = prefixedIri.trim();

    // If it's already a full IRI in angle brackets, unwrap it
    if (trimmed.startsWith('<') && trimmed.endsWith('>')) {
      return trimmed.slice(1, -1);
    }

    // If it contains a colon, try to expand the prefix
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
    // Try to find a matching prefix for this IRI
    for (const [prefix, namespace] of prefixes.entries()) {
      if (iri.startsWith(namespace)) {
        const localPart = iri.substring(namespace.length);
        return `${prefix}:${localPart}`;
      }
    }

    // If no prefix matches, wrap in angle brackets
    return `<${iri}>`;
  }
}
