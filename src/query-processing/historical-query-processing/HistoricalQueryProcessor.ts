import { WindowDefinition } from 'rsp-js';
import { ParsedJanusQuery } from '../live-stream-processing/JanusQLParser';

interface HistoricalStrategy {
  execute(queries: string[], metadata: WindowDefinition[]): HistoricalResults;
}

export class HistoricalQueryProcessor {
  constructor(
    private sparqlEndpoint: string,
    private strategy: HistoricalStrategy
  ) {}

  async process(parsedQuery: ParsedJanusQuery) {
    return this.strategy.execute(parsedQuery.sparqlQueries, parsedQuery.historicalWindows);
  }
}
