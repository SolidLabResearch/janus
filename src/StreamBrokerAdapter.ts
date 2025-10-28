import { Kafka, Producer, Consumer } from 'kafkajs';
import * as mqtt from 'mqtt';

export class StreamBrokerAdapter {
  private type: 'kafka' | 'mqtt';
  private broker: string;
  private kafkaProducer?: Producer;
  private kafkaConsumer?: Consumer;
  private mqttClient?: mqtt.MqttClient;

  constructor(brokerUrl: string) {
    const url = new URL(brokerUrl);
    this.type = url.protocol === 'kafka:' ? 'kafka' : 'mqtt';
    this.broker = `${url.hostname}:${url.port}`;
    if (this.type === 'kafka') {
      const kafka = new Kafka({
        clientId: 'janus',
        brokers: [this.broker],
      });
      this.kafkaProducer = kafka.producer();
      this.kafkaConsumer = kafka.consumer({ groupId: 'janus-group' });
    } else if (this.type === 'mqtt') {
      this.mqttClient = mqtt.connect(`mqtt://${this.broker}`);
    }
  }

  getBrokerLocation(): string {
    return this.broker;
  }

  async connect(): Promise<void> {
    if (this.type === 'kafka') {
      if (this.kafkaProducer) await this.kafkaProducer.connect();
      if (this.kafkaConsumer) await this.kafkaConsumer.connect();
    } else if (this.type === 'mqtt' && this.mqttClient) {
      return new Promise((resolve, reject) => {
        this.mqttClient!.on('connect', resolve);
        this.mqttClient!.on('error', reject);
      });
    }
  }

  async publish(topic: string, payload: any, callback: (error: any) => void): Promise<void> {
    try {
      if (this.type === 'kafka' && this.kafkaProducer) {
        await this.kafkaProducer.send({
          topic,
          messages: [{ value: JSON.stringify(payload) }],
        });
        callback(null);
      } else if (this.type === 'mqtt' && this.mqttClient) {
        this.mqttClient.publish(topic, JSON.stringify(payload), { qos: 0 }, (err?: Error) => {
          callback(err);
        });
      } else {
        callback(new Error('Unsupported broker type or not connected'));
      }
    } catch (error) {
      callback(error);
    }
  }

  async subscribe(topic: string, callback: (message: string) => void): Promise<void> {
    if (this.type === 'kafka' && this.kafkaConsumer) {
      await this.kafkaConsumer.subscribe({ topic, fromBeginning: false });
      await this.kafkaConsumer.run({
        eachMessage: async ({ message }) => {
          callback(message.value?.toString() || '');
        },
      });
    } else if (this.type === 'mqtt' && this.mqttClient) {
      this.mqttClient.subscribe(topic, { qos: 0 }, (err) => {
        if (err) throw err;
      });
      this.mqttClient.on('message', (receivedTopic, message) => {
        if (receivedTopic === topic) {
          callback(message.toString());
        }
      });
    }
  }
}
