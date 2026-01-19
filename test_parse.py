import jsonatapy

# Just try to compile and see if it parses
try:
    expr = jsonatapy.compile('function($'+'x) { $'+'x.price }')
    print('Lambda with field access parsed successfully!')
except Exception as e:
    print(f'Parse error: {e}')
