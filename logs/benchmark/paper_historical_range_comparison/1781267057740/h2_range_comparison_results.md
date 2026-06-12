# H2 Historical Timestamp Range Comparison

## Query

Both systems answer the same timestamp range query:

timestamp >= X
timestamp < Y

Janus executes this as a historical range lookup over its event-log structure.

Oxigraph executes this as SPARQL over the same RDF quads:

SELECT ?event ?t
WHERE {
  ?event <http://example.org/schema/timestamp> ?t .
  FILTER (?t >= X && ?t < Y)
}

## Result

| Query Case | 10k quads | 50k quads | 100k quads | 500k quads | Takeaway |
| --- | ---: | ---: | ---: | ---: | --- |
| Janus fixed 60s |  |  |  |  | Bounded timestamp lookup |
| Oxigraph fixed 60s FILTER |  |  |  |  | SPARQL timestamp filter |
| Janus full history |  |  |  |  | Full historical read |
| Oxigraph full history FILTER |  |  |  |  | Full timestamp-filter scan |

| Query Case | 10k | 50k | 100k | 500k |
| --- | --- | --- | --- | --- |
| fixed_60s_range | no | no | no | no |
| full_history_range | no | no | no | no |

## Interpretation

- fixed_60s_range tests bounded lookup over a 60-second historical interval
- full_history_range tests reading all historical data
- this directly compares Janus historical retrieval with Oxigraph SPARQL timestamp FILTER over the same RDF event log data
