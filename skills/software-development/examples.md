# Software Development Examples

This file contains approved examples of high-quality code patterns, design solutions, and before/after diffs.

## Example 1: Clear, Small Functions

### Good Pattern
Keep functions single-purpose and check inputs early:

```js
function calculateTotal(items) {
  if (!Array.isArray(items)) {
    throw new TypeError('Expected items to be an array');
  }
  return items.reduce((sum, item) => sum + (item.price * item.quantity), 0);
}
```
