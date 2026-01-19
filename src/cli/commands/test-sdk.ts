import { makeAnthropicCall } from '../../services/extraction/inference.js';
import { log } from '../../utils/log.js';

export async function testSdkCommand(): Promise<void> {
  log.info('test-sdk', 'Testing Agent SDK call...');
  console.log('Testing Agent SDK inference call...');

  const response = await makeAnthropicCall({
    model: 'haiku',
    max_tokens: 100,
    system: 'Respond with a brief JSON object.',
    messages: [
      {
        role: 'user',
        content: 'Say hello in JSON: {"greeting": "hello"}',
      },
    ],
  });

  if (response) {
    const text = response.content.find(c => c.type === 'text')?.text ?? '';
    console.log('SUCCESS! Response:', text.slice(0, 200));
    log.info('test-sdk', 'SDK call succeeded', {
      responseLength: text.length,
      inputTokens: response.usage?.input_tokens,
      outputTokens: response.usage?.output_tokens,
    });
  } else {
    console.error('FAILED: No response from SDK');
    log.error('test-sdk', 'SDK call failed - no response');
    process.exit(1);
  }
}
