import axios from 'axios';
import { JenaAdapter } from './JenaAdapter';
import { RdfFormat, RdfTermType } from '../core/types';

// Mock axios
jest.mock('axios');
const mockedAxios = axios as jest.Mocked<typeof axios>;

// Mock axios.isAxiosError to check for isAxiosError property
mockedAxios.isAxiosError.mockImplementation((error: any) => error && error.isAxiosError === true);

describe('JenaAdapter', () => {
  let adapter: JenaAdapter;
  let mockClient: any;
  const mockEndpoint = {
    url: 'http://localhost:3030',
    storeType: 'jena' as const,
    timeoutSecs: 30,
  };

  const createAdapterWithMock = (endpoint = mockEndpoint, dataset = 'testDataset') => {
    const mockClient = {
      post: jest.fn(),
      get: jest.fn(),
      interceptors: { request: { use: jest.fn() } },
    };
    mockedAxios.create.mockReturnValue(mockClient as any);
    const adapter = new JenaAdapter(endpoint, dataset);
    return { adapter, mockClient };
  };

  beforeEach(() => {
    // Clear all mocks
    jest.clearAllMocks();

    // Create adapter with default mock
    const res = createAdapterWithMock();
    adapter = res.adapter;
    mockClient = res.mockClient;
  });

  describe('constructor', () => {
    it('should create adapter with correct configuration', () => {
      const endpointWithAuth = {
        ...mockEndpoint,
        authToken: 'test-token',
      };

      const { adapter: adapterWithAuth } = createAdapterWithMock(endpointWithAuth);

      expect(mockedAxios.create).toHaveBeenCalledWith({
        baseURL: 'http://localhost:3030',
        timeout: 30000,
        headers: {},
      });
    });

    it('should add authorization header when auth token is provided', () => {
      const endpointWithAuth = {
        ...mockEndpoint,
        authToken: 'test-token',
      };

      const { adapter: adapterWithAuth, mockClient: authMockClient } =
        createAdapterWithMock(endpointWithAuth);

      // The interceptor should be set up
      expect(authMockClient.interceptors.request.use).toHaveBeenCalled();
    });
  });

  describe('loadData', () => {
    it('should load Turtle data successfully', async () => {
      mockClient.post.mockResolvedValue({ status: 200 });

      const turtleData = '@prefix ex: <http://example.org/> . ex:Alice ex:knows ex:Bob .';
      const result = await adapter.loadData(turtleData, RdfFormat.Turtle);

      expect(mockClient.post).toHaveBeenCalledWith('/testDataset/data', turtleData, {
        headers: { 'Content-Type': 'text/turtle' },
      });
      expect(result).toBe(-1); // Jena doesn't return count
    });

    it('should load data into named graph', async () => {
      mockClient.post.mockResolvedValue({ status: 200 });

      const turtleData = '@prefix ex: <http://example.org/> . ex:Alice ex:knows ex:Bob .';
      await adapter.loadData(turtleData, RdfFormat.Turtle, 'http://example.org/graph1');

      expect(mockClient.post).toHaveBeenCalledWith(
        '/testDataset/data?graph=http%3A%2F%2Fexample.org%2Fgraph1',
        turtleData,
        { headers: { 'Content-Type': 'text/turtle' } }
      );
    });

    it('should handle different RDF formats', async () => {
      const formats = [
        { format: RdfFormat.NTriples, contentType: 'application/n-triples' },
        { format: RdfFormat.RdfXml, contentType: 'application/rdf+xml' },
        { format: RdfFormat.JsonLd, contentType: 'application/ld+json' },
      ];

      for (const { format, contentType } of formats) {
        mockClient.post.mockResolvedValue({ status: 200 });

        await adapter.loadData(
          '<http://example.org/s> <http://example.org/p> <http://example.org/o> .',
          format
        );
        expect(mockClient.post).toHaveBeenCalledWith('/testDataset/data', expect.any(String), {
          headers: { 'Content-Type': contentType },
        });
      }
    });

    it('should throw error on HTTP failure', async () => {
      mockClient.post.mockRejectedValue(
        Object.assign(new Error('Mock error'), {
          isAxiosError: true,
          response: { status: 500, data: { message: 'Internal Server Error' } },
        })
      );

      await expect(adapter.loadData('invalid data', RdfFormat.Turtle)).rejects.toThrow(
        'Server error: Internal Server Error'
      );
    });
  });

  describe('query', () => {
    it('should execute SELECT query successfully', async () => {
      const mockResponse = {
        data: {
          head: { vars: ['s', 'p', 'o'] },
          results: {
            bindings: [
              {
                s: { type: 'uri', value: 'http://example.org/Alice' },
                p: { type: 'uri', value: 'http://example.org/knows' },
                o: { type: 'uri', value: 'http://example.org/Bob' },
              },
            ],
          },
        },
      };

      mockClient.post.mockResolvedValue(mockResponse);

      const query = 'SELECT * WHERE { ?s ?p ?o }';
      const result = await adapter.query(query);

      expect(mockClient.post).toHaveBeenCalledWith(
        '/testDataset/sparql',
        expect.any(URLSearchParams),
        expect.objectContaining({
          headers: {
            'Content-Type': 'application/x-www-form-urlencoded',
            Accept: 'application/sparql-results+json',
          },
        })
      );

      expect(result).toHaveProperty('head.vars', ['s', 'p', 'o']);
      expect(result).toHaveProperty('results.bindings');
    });

    it('should execute ASK query successfully', async () => {
      const mockResponse = {
        data: {
          head: {},
          boolean: true,
        },
      };

      mockClient.post.mockResolvedValue(mockResponse);

      const query = 'ASK { ?s ?p ?o }';
      const result = await adapter.query(query);

      expect(result).toHaveProperty('boolean', true);
    });

    it('should execute CONSTRUCT query successfully', async () => {
      const mockResponse = {
        data: {
          triples: [
            {
              subject: { type: 'uri', value: 'http://example.org/Alice' },
              predicate: { type: 'uri', value: 'http://example.org/knows' },
              object: { type: 'uri', value: 'http://example.org/Bob' },
            },
          ],
        },
      };

      mockClient.post.mockResolvedValue(mockResponse);

      const query = 'CONSTRUCT { ?s ?p ?o } WHERE { ?s ?p ?o }';
      const result = await adapter.query(query);

      expect(result).toHaveProperty('triples');
      expect(result.triples).toHaveLength(1);
    });

    it('should handle query options', async () => {
      const mockResponse = {
        data: {
          head: { vars: ['s'] },
          results: { bindings: [] },
        },
      };

      mockClient.post.mockResolvedValue(mockResponse);

      const query = 'SELECT ?s WHERE { ?s ?p ?o }';
      const options = {
        defaultGraphUri: 'http://example.org/graph',
        namedGraphUris: ['http://example.org/graph1', 'http://example.org/graph2'],
        timeout: 5000,
      };

      await adapter.query(query, options);

      expect(mockClient.post).toHaveBeenCalledWith(
        '/testDataset/sparql',
        expect.any(URLSearchParams),
        expect.objectContaining({
          params: {
            'default-graph-uri': 'http://example.org/graph',
            'named-graph-uri': ['http://example.org/graph1', 'http://example.org/graph2'],
            timeout: '5000',
          },
        })
      );
    });
  });

  describe('insert', () => {
    it('should insert triple successfully', async () => {
      mockClient.post.mockResolvedValue({ status: 200 });

      const triple = {
        subject: { type: RdfTermType.Uri, value: 'http://example.org/Alice' },
        predicate: { type: RdfTermType.Uri, value: 'http://example.org/name' },
        object: { type: RdfTermType.Literal, value: 'Alice' },
      };

      await adapter.insert(triple);

      expect(mockClient.post).toHaveBeenCalledWith(
        '/testDataset/update',
        expect.any(URLSearchParams),
        {
          headers: { 'Content-Type': 'application/x-www-form-urlencoded' },
        }
      );
    });

    it('should insert triple into named graph', async () => {
      mockClient.post.mockResolvedValue({ status: 200 });

      const triple = {
        subject: { type: RdfTermType.Uri, value: 'http://example.org/Alice' },
        predicate: { type: RdfTermType.Uri, value: 'http://example.org/name' },
        object: { type: RdfTermType.Literal, value: 'Alice' },
        graph: { type: RdfTermType.Uri, value: 'http://example.org/graph1' },
      };

      await adapter.insert(triple);

      expect(mockClient.post).toHaveBeenCalledWith(
        '/testDataset/update',
        expect.any(URLSearchParams),
        {
          headers: { 'Content-Type': 'application/x-www-form-urlencoded' },
        }
      );
    });
  });

  describe('remove', () => {
    it('should remove triple successfully', async () => {
      mockClient.post.mockResolvedValue({ status: 200 });

      const triple = {
        subject: { type: RdfTermType.Uri, value: 'http://example.org/Alice' },
        predicate: { type: RdfTermType.Uri, value: 'http://example.org/name' },
        object: { type: RdfTermType.Literal, value: 'Alice' },
      };

      await adapter.remove(triple);

      expect(mockClient.post).toHaveBeenCalledWith(
        '/testDataset/update',
        expect.any(URLSearchParams),
        {
          headers: { 'Content-Type': 'application/x-www-form-urlencoded' },
        }
      );
    });
  });

  describe('size', () => {
    it('should return triple count', async () => {
      const mockResponse = {
        data: {
          head: { vars: ['count'] },
          results: {
            bindings: [{ count: { value: '42' } }],
          },
        },
      };

      mockClient.post.mockResolvedValue(mockResponse);

      const size = await adapter.size();
      expect(size).toBe(42);
    });

    it('should return 0 when no results', async () => {
      const mockResponse = {
        data: {
          head: { vars: ['count'] },
          results: { bindings: [] },
        },
      };

      mockClient.post.mockResolvedValue(mockResponse);

      const size = await adapter.size();
      expect(size).toBe(0);
    });
  });

  describe('clear', () => {
    it('should clear store successfully', async () => {
      mockClient.post.mockResolvedValue({ status: 200 });

      await adapter.clear();

      expect(mockClient.post).toHaveBeenCalledWith(
        '/testDataset/update',
        expect.any(URLSearchParams),
        {
          headers: { 'Content-Type': 'application/x-www-form-urlencoded' },
        }
      );
    });
  });

  describe('export', () => {
    it('should export data in specified format', async () => {
      const mockResponse = {
        data: '@prefix ex: <http://example.org/> . ex:Alice ex:knows ex:Bob .',
      };

      mockClient.get.mockResolvedValue(mockResponse);

      const result = await adapter.export(RdfFormat.Turtle);

      expect(mockClient.get).toHaveBeenCalledWith('/testDataset/data', {
        headers: { Accept: 'text/turtle' },
      });
      expect(result).toBe(mockResponse.data);
    });
  });

  describe('contains', () => {
    it('should return true when triple exists', async () => {
      const mockResponse = {
        data: { head: {}, boolean: true },
      };

      mockClient.post.mockResolvedValue(mockResponse);

      const triple = {
        subject: { type: RdfTermType.Uri, value: 'http://example.org/Alice' },
        predicate: { type: RdfTermType.Uri, value: 'http://example.org/knows' },
        object: { type: RdfTermType.Uri, value: 'http://example.org/Bob' },
      };

      const result = await adapter.contains(triple);
      expect(result).toBe(true);
    });

    it('should return false when triple does not exist', async () => {
      const mockResponse = {
        data: { head: {}, boolean: false },
      };

      mockClient.post.mockResolvedValue(mockResponse);

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
      mockClient.get.mockResolvedValue({ status: 200 });

      const result = await adapter.ping();
      expect(result).toBe(true);
      expect(mockClient.get).toHaveBeenCalledWith('/$/ping');
    });

    it('should return false when server is unavailable', async () => {
      mockClient.get.mockRejectedValue(new Error('Connection failed'));

      const result = await adapter.ping();
      expect(result).toBe(false);
    });
  });

  describe('error handling', () => {
    it('should handle 404 errors', async () => {
      mockClient.post.mockRejectedValue(
        Object.assign(new Error('Mock error'), {
          isAxiosError: true,
          response: { status: 404, data: { message: 'Dataset not found' } },
        })
      );

      await expect(adapter.loadData('test', RdfFormat.Turtle)).rejects.toThrow(
        'Resource not found: Dataset not found'
      );
    });

    it('should handle authentication errors', async () => {
      mockClient.post.mockRejectedValue(
        Object.assign(new Error('Mock error'), {
          isAxiosError: true,
          response: { status: 401, data: { message: 'Unauthorized' } },
        })
      );

      await expect(adapter.loadData('test', RdfFormat.Turtle)).rejects.toThrow(
        'Authentication error (401): Unauthorized'
      );
    });

    it('should handle generic errors', async () => {
      mockClient.post.mockRejectedValue(new Error('Network error'));

      await expect(adapter.loadData('test', RdfFormat.Turtle)).rejects.toThrow('Network error');
    });
  });
});
