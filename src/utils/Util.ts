import { Store, Writer } from 'n3';
import rdfParse from 'rdf-parse';
import { storeStream } from 'rdf-store-stream';
import streamifyString from 'streamify-string';
import { createHash } from 'crypto';

interface ParseOptions {
  contentType: string;
  baseIRI?: string;
}

export async function turtleStringToStore(text: string, baseIRI?: string): Promise<Store> {
  return await stringToStore(text, { contentType: 'text/turtle', baseIRI });
}

export async function stringToStore(text: string, options: ParseOptions): Promise<Store> {
  const stream = streamifyString(text);
  const quadStream = rdfParse.parse(stream, {
    contentType: options.contentType,
    baseIRI: options.baseIRI,
  });
  const store = await storeStream(quadStream);
  return store as Store;
}

export function storeToString(store: Store) : string { 
  const writer = new Writer();
  return writer.quadsToString(store.getQuads(null, null, null, null));
}

export function hashStringMD5(input: string): string { 
  input = input.replace(/\s/g, '');
  const hash = createHash('md5');
  hash.update(input);
  return hash.digest('hex');
}
