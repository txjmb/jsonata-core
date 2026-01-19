import jsonatapy
import time

# Test 1: Simple array index
data1 = {"arr": list(range(1000))}
expr1 = jsonatapy.compile('arr[500]')

start = time.perf_counter()
for _ in range(1000):
    result = expr1.evaluate(data1)
elapsed = time.perf_counter() - start

print(f"Test 1: arr[500] on 1000-element array")
print(f"  1000 iterations: {elapsed*1000:.2f}ms")
print(f"  Per iteration: {elapsed*1000000:.2f}µs")
print(f"  Result: {result}")
print()

# Test 2: Array sum
expr2 = jsonatapy.compile('$sum(arr)')
start = time.perf_counter()
for _ in range(100):
    result = expr2.evaluate(data1)
elapsed = time.perf_counter() - start

print(f"Test 2: $sum(arr) on 1000-element array")
print(f"  100 iterations: {elapsed*1000:.2f}ms")
print(f"  Per iteration: {elapsed*1000:.2f}ms")
print(f"  Result: {result}")
print()

# Test 3: Field access on array of objects
data3 = {"items": [{"id": i, "value": i*2} for i in range(100)]}
expr3 = jsonatapy.compile('items.value')
start = time.perf_counter()
for _ in range(100):
    result = expr3.evaluate(data3)
elapsed = time.perf_counter() - start

print(f"Test 3: items.value on 100-element array")
print(f"  100 iterations: {elapsed*1000:.2f}ms")
print(f"  Per iteration: {elapsed*1000:.2f}ms")
print(f"  Result length: {len(result)}")
