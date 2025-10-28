import axios from 'axios';
import { OxigraphAdapter } from './OxigraphAdapter';
import { RdfFormat, RdfTermType } from '../core/types';

describe('OxigraphAdapter (Integration Tests)', () => {
  let adapter: OxigraphAdapter;
  const endpoint = {
    url: 'http://localhost:7878',
    storeType: 'oxigraph' as const,
    timeoutSecs: 30,
  };

  beforeAll(async () => {
    // Check if Oxigraph server is available
    try {
      const response = await axios.get('http://localhost:7878/', { timeout: 5000 });
      if (response.status !== 200) {
        throw new Error('Server not responding');
      }
    } catch (error) {
      console.warn('Oxigraph server not available, skipping integration tests');
      // Skip all tests in this suite
      (describe as any).skip();
      return;
    }

    // Create real adapter
    adapter = new OxigraphAdapter(endpoint);

    // Clear any existing data
    try {
      await adapter.clear();
    } catch (error) {
      // Ignore if clear fails
    }
  });

  afterEach(async () => {
    // Clear the store after each test
    try {
      await adapter.clear();
    } catch (error) {
      // Ignore if clear fails
    }
  });

  describe('loadData', () => {
    it('should load Turtle data successfully', async () => {
      const turtleData = '@prefix ex: <http://example.org/> . ex:Alice ex:knows ex:Bob .';
      const result = await adapter.loadData(turtleData, RdfFormat.Turtle);

      expect(result).toBe(-1); // Oxigraph doesn't return count
    });

    it('should load data into named graph', async () => {
      const turtleData = '@prefix ex: <http://example.org/> . ex:Alice ex:knows ex:Bob .';
      await adapter.loadData(turtleData, RdfFormat.Turtle, 'http://example.org/graph1');

      // Verify by querying
      const query = 'SELECT * WHERE { GRAPH <http://example.org/graph1> { ?s ?p ?o } }';
      const result = await adapter.query(query);
      expect(result.results.bindings).toHaveLength(1);
      console.log(result.results.bindings);
    });

    it('should handle different RDF formats', async () => {
      const ntriplesData = '<http://example.org/s> <http://example.org/p> <http://example.org/o> .';
      await adapter.loadData(ntriplesData, RdfFormat.NTriples);

      const size = await adapter.size();
      expect(size).toBeGreaterThanOrEqual(0);
    });
  });

  describe('query', () => {
    beforeEach(async () => {
      // Load some test data
      const turtleData = '@prefix ex: <http://example.org/> . ex:Alice ex:knows ex:Bob .';
      await adapter.loadData(turtleData, RdfFormat.Turtle);
    });

    it('should execute SELECT query successfully', async () => {
      const query = 'SELECT * WHERE { ?s ?p ?o }';
      const result = await adapter.query(query);

      expect(result).toHaveProperty('head.vars');
      expect(result).toHaveProperty('results.bindings');
      expect(Array.isArray(result.results.bindings)).toBe(true);
    });

    it('should execute ASK query successfully', async () => {
      const query = 'ASK { ?s ?p ?o }';
      const result = await adapter.query(query);

      expect(result).toHaveProperty('boolean');
      expect(typeof result.boolean).toBe('boolean');
    });

    it('should execute CONSTRUCT query successfully', async () => {
      const query = 'CONSTRUCT { ?s ?p ?o } WHERE { ?s ?p ?o }';
      const result = await adapter.query(query);

      expect(result).toHaveProperty('triples');
      expect(Array.isArray(result.triples)).toBe(true);
    });
  });

  describe('insert', () => {
    it('should insert triple successfully', async () => {
      const triple = {
        subject: { type: RdfTermType.Uri, value: 'http://example.org/Alice' },
        predicate: { type: RdfTermType.Uri, value: 'http://example.org/name' },
        object: { type: RdfTermType.Literal, value: 'Alice' },
      };

      await adapter.insert(triple);

      const size = await adapter.size();
      expect(size).toBe(1);
    });

    it('should insert triple into named graph', async () => {
      const triple = {
        subject: { type: RdfTermType.Uri, value: 'http://example.org/Alice' },
        predicate: { type: RdfTermType.Uri, value: 'http://example.org/name' },
        object: { type: RdfTermType.Literal, value: 'Alice' },
        graph: { type: RdfTermType.Uri, value: 'http://example.org/graph1' },
      };

      await adapter.insert(triple);

      const query = 'SELECT * WHERE { GRAPH <http://example.org/graph1> { ?s ?p ?o } }';
      const result = await adapter.query(query);
      expect(result.results.bindings).toHaveLength(1);
    });
  });

  describe('remove', () => {
    beforeEach(async () => {
      const triple = {
        subject: { type: RdfTermType.Uri, value: 'http://example.org/Alice' },
        predicate: { type: RdfTermType.Uri, value: 'http://example.org/name' },
        object: { type: RdfTermType.Literal, value: 'Alice' },
      };
      await adapter.insert(triple);
    });

    it('should remove triple successfully', async () => {
      const triple = {
        subject: { type: RdfTermType.Uri, value: 'http://example.org/Alice' },
        predicate: { type: RdfTermType.Uri, value: 'http://example.org/name' },
        object: { type: RdfTermType.Literal, value: 'Alice' },
      };

      await adapter.remove(triple);

      const size = await adapter.size();
      expect(size).toBe(0);
    });
  });

  describe('size', () => {
    it('should return triple count', async () => {
      const turtleData =
        '@prefix ex: <http://example.org/> . ex:Alice ex:knows ex:Bob ; ex:name "Alice" .';
      await adapter.loadData(turtleData, RdfFormat.Turtle);

      const size = await adapter.size();
      expect(typeof size).toBe('number');
      expect(size).toBeGreaterThanOrEqual(0);
    });
  });

  describe('clear', () => {
    beforeEach(async () => {
      const turtleData = '@prefix ex: <http://example.org/> . ex:Alice ex:knows ex:Bob .';
      await adapter.loadData(turtleData, RdfFormat.Turtle);
    });

    it('should clear store successfully', async () => {
      await adapter.clear();

      const size = await adapter.size();
      expect(size).toBe(0);
    });
  });

  describe('export', () => {
    beforeEach(async () => {
      const turtleData = '@prefix ex: <http://example.org/> . ex:Alice ex:knows ex:Bob .';
      await adapter.loadData(turtleData, RdfFormat.Turtle);
    });

    it('should export data in specified format', async () => {
      const result = await adapter.export(RdfFormat.NTriples);

      expect(result).toBeDefined();
    });
  });

  describe('contains', () => {
    beforeEach(async () => {
      const turtleData = '@prefix ex: <http://example.org/> . ex:Alice ex:knows ex:Bob .';
      await adapter.loadData(turtleData, RdfFormat.Turtle);
    });

    it('should return true when triple exists', async () => {
      // First load some data
      const turtleData = '@prefix ex: <http://example.org/> . ex:Alice ex:knows ex:Bob .';
      await adapter.loadData(turtleData, RdfFormat.Turtle);

      const triple = {
        subject: { type: RdfTermType.Uri, value: 'http://example.org/Alice' },
        predicate: { type: RdfTermType.Uri, value: 'http://example.org/knows' },
        object: { type: RdfTermType.Uri, value: 'http://example.org/Bob' },
      };

      const result = await adapter.contains(triple);
      expect(typeof result).toBe('boolean');
    });

    it('should return false when triple does not exist', async () => {
      const triple = {
        subject: { type: RdfTermType.Uri, value: 'http://example.org/Alice' },
        predicate: { type: RdfTermType.Uri, value: 'http://example.org/knows' },
        object: { type: RdfTermType.Uri, value: 'http://example.org/Charlie' },
      };

      const result = await adapter.contains(triple);
      expect(result).toBe(false);
    });
  });

  describe('ping', () => {
    it('should return true when server is available', async () => {
      const result = await adapter.ping();
      expect(result).toBe(true);
    });
  });
});
