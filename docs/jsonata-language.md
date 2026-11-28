# JSONata Language Reference

jsonatapy implements the [JSONata](https://jsonata.org/) query and transformation language for JSON data.

## What is JSONata?

JSONata is a lightweight query and transformation language for JSON data. Inspired by the 'location path' semantics of XPath 3.1, it allows sophisticated queries to be expressed in a compact and intuitive notation.

## Language Features

### Path Expressions

Navigate through JSON structures using dot notation:

```jsonata
user.name
user.address.city
products[0].name
```

### Predicates

Filter arrays using boolean conditions in square brackets:

```jsonata
products[price > 100]
users[age >= 18 and status = 'active']
```

### Array Operations

- **Mapping**: `products.name` - extract all names
- **Filtering**: `products[inStock]` - filter by condition
- **Flattening**: Automatically flattens nested arrays

### Operators

**Comparison**: `=`, `!=`, `<`, `<=`, `>`, `>=`

**Logical**: `and`, `or`, `not`

**Arithmetic**: `+`, `-`, `*`, `/`, `%`

**String concatenation**: `&` (see also [template strings](#template-strings))

**Range**: `[1..5]` - generates `[1, 2, 3, 4, 5]`

### Template Strings

New in JSONata 2.3.0. A template string builds a string from literal text and
embedded expressions in curly braces. It is enclosed in guillemets (`«` and `»`)
or, the same thing, in double angle brackets (`<<` and `>>`):

```jsonata
«Hello {firstName} {lastName}, you have {$count(orders)} orders»
```

Each embedded value is cast to a string as the `&` operator casts it, so a
missing value adds nothing and `{}` adds nothing. Any expression can go inside
the braces, including function calls, object constructors and other template
strings; a sequence separated by `;` needs its own parentheses, as in
`«{($x := 1; $x + 1)}»`.

A template is closed by the delimiter that matches its opener, so `<<` and `>>`
are ordinary text inside `«...»`, and the guillemets inside `<<...>>`. A
`<<...>>` template ends at the first `>>`, so text that ends in `>` needs an
escape: `<<<li>{name}</li\>>>`. The text takes the escapes of a string
literal, plus `\{`, `\}`, `\«`, `\»`, `\<` and `\>`; an unescaped `}` in the
text is an error (S0108).

- `«{1 + 2} apples»` gives `"3 apples"`
- `«{missing}»` gives `""`
- `«\{literal\}»` gives `"{literal}"`
- `<<a \>> b>>` gives `"a >> b"`

### Object Construction

Create new JSON objects:

```jsonata
{
    "fullName": firstName & " " & lastName,
    "age": age,
    "isAdult": age >= 18
}
```

### Array Constructors

Create new arrays:

```jsonata
[1, 2, 3]
[name, age, email]
```

### Built-in Functions

#### String Functions
- `$string(arg)` - Cast to string
- `$length(str)` - String length
- `$substring(str, start, length?)` - Extract substring
- `$uppercase(str)` - Convert to uppercase
- `$lowercase(str)` - Convert to lowercase
- `$trim(str)` - Remove whitespace
- `$contains(str, pattern)` - Check if contains pattern
- `$split(str, separator, limit?)` - Split string
- `$join(array, separator?)` - Join array elements
- `$replace(str, pattern, replacement)` - Replace pattern

#### Numeric Functions
- `$number(arg)` - Cast to number
- `$abs(number)` - Absolute value
- `$floor(number)` - Round down
- `$ceil(number)` - Round up
- `$round(number, precision?)` - Round to precision
- `$power(base, exponent)` - Exponentiation
- `$sqrt(number)` - Square root
- `$random()` - Random number [0, 1)

#### Array Functions
- `$count(array)` - Count elements
- `$sum(array)` - Sum numeric elements
- `$max(array)` - Maximum value
- `$min(array)` - Minimum value
- `$average(array)` - Average value
- `$append(array1, array2)` - Concatenate arrays
- `$reverse(array)` - Reverse array
- `$sort(array, function?)` - Sort array
- `$distinct(array)` - Remove duplicates
- `$shuffle(array)` - Randomly shuffle

#### Object Functions
- `$keys(object)` - Get object keys
- `$lookup(object, key)` - Get value by key
- `$spread(object)` - Spread object into array
- `$merge(array)` - Merge objects in array
- `$sift(object, function)` - Filter object properties

#### Higher-Order Functions
- `$map(array, function)` - Transform each element
- `$filter(array, function)` - Filter elements
- `$reduce(array, function, init?)` - Reduce to single value
- `$each(object, function)` - Iterate over object properties

#### Boolean Functions
- `$boolean(arg)` - Cast to boolean
- `$not(arg)` - Logical NOT
- `$exists(arg)` - Check if exists

#### Other Functions
- `$type(value)` - Get type of value
- `$assert(condition, message?)` - Assertion

### Lambda Functions

Define anonymous functions:

```jsonata
function($x) { $x * 2 }
function($x, $y) { $x + $y }
```

Use with higher-order functions:

```jsonata
$map(numbers, function($n) { $n * $n })
$filter(products, function($p) { $p.price > 100 })
```

### Variable Binding

Bind values to variables for reuse:

```jsonata
$total := $sum(items.price);
$tax := $total * 0.1;
$total + $tax
```

### Conditional Expressions

Ternary operator:

```jsonata
age >= 18 ? "Adult" : "Minor"
```

Nested conditionals:

```jsonata
score >= 90 ? "A" :
score >= 80 ? "B" :
score >= 70 ? "C" : "F"
```

### Parent Operator

Use `%` to reference parent context in predicates:

```jsonata
products[price > %.averagePrice]
```

### Wildcards

- `*` - All properties
- `**` - Recursive descent (all nested properties)

```jsonata
account.*.balance
account.**.balance
```

## Type System

JSONata has the following types:

- **string** - Text
- **number** - Numeric values (64-bit float)
- **boolean** - `true` or `false`
- **null** - Null value
- **array** - Ordered collection
- **object** - Key-value pairs
- **function** - Lambda functions

## Compatibility

jsonatapy aims for 100% compatibility with the reference JavaScript implementation. Currently:

- **1972/1972 tests passing**
- All language features supported
- All built-in functions implemented
- Full lambda and higher-order function support

See [Compatibility Status](compatibility.md) for detailed information.

## Learn More

- [Official JSONata Documentation](https://docs.jsonata.org/)
- [JSONata Exerciser](https://try.jsonata.org/) - Interactive playground
- [Examples](examples.md) - Practical examples
- [API Reference](api.md) - jsonatapy API documentation

## Differences from JavaScript Implementation

jsonatapy is implemented in Rust for performance, but maintains semantic compatibility with the JavaScript reference implementation:

- **Performance**: Typically faster than JavaScript for most operations
- **Type handling**: Same type coercion rules as JavaScript
- **Error messages**: Similar error messages and stack traces
- **Async**: Currently synchronous only (async support planned)

See [Performance](performance.md) for benchmarks and optimization tips.
