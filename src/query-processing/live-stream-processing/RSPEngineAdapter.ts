import { StreamBrokerAdapter } from '@/StreamBrokerAdapter';
import { DataFactory, Quad } from 'n3';
import { randomUUID } from 'node:crypto';
import { EventEmitter } from 'node:stream';
import { turtleStringToStore } from '@/utils/Util';
import { RDFStream, RSPEngine, RSPQLParser } from 'rsp-js';

/**
 * RSP Engine adapter for processing RDF Streams
 */
export class RSPEngineAdapter {
  private query: string;
  private rstream_topic: string;
  private rstream_emitter: EventEmitter;
  private rsp_engine: RSPEngine;
  private rspql_parser: RSPQLParser;
  private adapters: Map<string, StreamBrokerAdapter> = new Map();

  /**
   * @param {string} rspql_query - The RSP-QL query for the live event stream querying process.
   * @param {string} rstream_topic - The topic name of the RDF Stream where the RDF triples are published.
   */
  constructor(rspql_query: string, rstream_topic: string) {
    this.query = rspql_query;
    this.rstream_topic = rstream_topic;
    this.rsp_engine = new RSPEngine(rspql_query);
    this.rstream_emitter = this.rsp_engine.register();
    this.rspql_parser = new RSPQLParser();
  }

  public async processRDFStreams() {
    if (!this.query || this.query.trim() === '') {
      throw new Error('RSP-QL query is not defined or is empty.');
    }

    const parsed_query = this.rspql_parser.parse(this.query);
    if (parsed_query) {
      const streams: any[] = [...parsed_query.s2r];
      for (const stream of streams) {
        const stream_name = stream.stream_name;
        const url_stream = new URL(stream_name);
        const rsp_stream = this.rsp_engine.getStream(stream_name);
        if (!rsp_stream) {
          throw new Error(`Stream ${stream_name} is not defined in the RSP-QL query.`);
        }
        const brokerUrl = `${url_stream.protocol}//${url_stream.hostname}:${url_stream.port}`;
        const topic = url_stream.pathname.slice(1);

        if (!this.adapters.has(brokerUrl)) {
          this.adapters.set(brokerUrl, new StreamBrokerAdapter(brokerUrl));
        }
        const adapter = this.adapters.get(brokerUrl)!;
        await adapter.connect();
        await adapter.subscribe(topic, async (message: string) => {
          if (!message || message.length === 0) {
            throw new Error(`Received empty message on the topic ${topic}`);
          }

          try {
            const event_store = await turtleStringToStore(message.toString());
            const event_timestamp = event_store.getQuads(
              null,
              DataFactory.namedNode('https://saref.etsi.org/core/hasTimestamp'),
              null,
              null
            )[0]?.object.value;

            if (!event_timestamp) {
              throw new Error(
                `No timestamp found in the RDF message on topic ${topic}. Maybe the message is not properly annotated. Currently we utilise the saref:hasTimestamp annotation for extracting the event timestamp.`
              );
            }

            const timestamp_epoch = Date.parse(event_timestamp);
            if (rsp_stream) {
              await this.addStoreToRSPEngine(event_store, [rsp_stream], timestamp_epoch);
            } else {
              throw new Error(`Stream ${stream_name} not found in RSP Engine.`);
            }
          } catch (error) {
            throw new Error(`Failed to parse RDF message on topic ${topic}: ${error}`);
          }
        });
      }
    } else {
      throw new Error('Failed to parse the RSP-QL query.');
    }
  }

  public async subscribeToResultStream(brokerUrl: string) {
    console.log(`Subscribing to the result stream: ${this.rstream_topic}`);
    if (!this.rstream_topic || this.rstream_topic.trim() === '') {
      throw new Error('RStream topic is not defined or is empty.');
    }

    if (!this.adapters.has(brokerUrl)) {
      this.adapters.set(brokerUrl, new StreamBrokerAdapter(brokerUrl));
    }
    const adapter = this.adapters.get(brokerUrl)!;
    await adapter.connect();

    this.rstream_emitter.on('RStream', async (object: any) => {
      if (!object || !object.bindings) {
        throw new Error('The query resulted into no bindings and results from the window');
      }

      const iterables = object.bindings.values();

      for await (const iterable of iterables) {
        const event_timestamp = new Date().getTime();
        const data = iterable.value;

        const annotated_result_event = this.generateAnnotatedResultEvent(data, event_timestamp);
        const payload = JSON.stringify(annotated_result_event);
        adapter.publish(this.rstream_topic, payload, (error: any) => {
          if (error) {
            throw new Error(
              `Failed to publish annotated result event to topic ${this.rstream_topic}: ${error.message}`
            );
          } else {
            console.log(
              `Published annotated result event to topic ${this.rstream_topic}: ${payload}`
            );
          }
        });
      }
    });
  }

  private generateAnnotatedResultEvent(data: any, timestamp: number) {
    const random_uuid = randomUUID();
    const annotated_event = `<https://rsp.js/aggregation_event/${random_uuid}> <https://saref.etsi.org/core/hasValue> "${data}"^^<http://www.w3.org/2001/XMLSchema#float> .
                            <https://rsp.js/aggregation_event/${random_uuid}> <https://saref.etsi.org/core/hasTimestamp> "${timestamp}"^^<http://www.w3.org/2001/XMLSchema#long> .
      `;
    return annotated_event.trim();
  }

  private async addStoreToRSPEngine(store: any, stream_name: RDFStream[], timestamp: number) {
    const quads = store.getQuads(null, null, null, null);
    for (const stream of stream_name) {
      const quadSet = new Set<Quad>();
      for (const quad of quads) {
        const quadWithGraph = DataFactory.quad(
          quad.subject,
          quad.predicate,
          quad.object,
          DataFactory.namedNode(stream.name)
        );
        quadSet.add(quadWithGraph);
      }
      stream.add(quadSet, timestamp);
    }
  }
}
