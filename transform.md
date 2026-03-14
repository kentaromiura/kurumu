First, this is a Babel plugin. Babel plugins transform JavaScript code. Looking at the structure, it exports a function that takes babel as an argument and returns a plugin object with a visitor.
The visitor is looking for CallExpression nodes - that means function calls in the code.
Specifically, it's looking for calls where:
- The callee is a MemberExpression (like object.property)
- The property name is 'css'
- The object name is 'Html'
So it's targeting calls like Html.css(...)
When such a call is found, it does several things:
1. Sets up some variables: content array, originalCode array, units array, and a uniqueId (incrementing counter)
2. Creates a mock Html object with a css method that uses @emotion/css to process CSS
3. Depending on whether there are template literal arguments (the second argument to Html.css), it either:
   - Directly evaluates the path if no arguments (line 40)
   - Or replaces the template literal expressions with CSS variable placeholders (lines 42-45), then evaluates
4. Finds the parent ObjectExpression (likely a style object in a React component)
5. If the object already has a 'style' property:
   - It extracts the original CSS values
   - For each value, it adds a CSS variable property to the style object (like --kentacss0: value)
   - It preserves the original values with units
6. If there's no style property:
   - It creates a new style object with CSS variable properties
7. If no parent ObjectExpression is found (meaning it's not inside an object):
   - It creates DOM manipulation code that sets CSS variables on document.body
   - Uses template.expression to create a statement that runs on DOMContentLoaded and immediately
8. After processing, it:
   - Collects all inserted CSS from emotion cache
   - Replaces the original Html.css(...) call with just the resulting CSS string
   - Writes all collected CSS to out.css file
The main purpose seems to be extracting CSS-in-JS (specifically using emotion's css function via Html.css) into actual CSS files, while replacing the JS calls with CSS variable references that can be applied to the DOM.
Let me trace through a concrete example:
Suppose we have:
const style = Html.css`
  color: red;
  font-size: 16px;
`;
This would match the visitor. The arguments would be:
- arguments0: template literal parts "color: red;\n  font-size: 16px;", ""
- arguments1: empty array (no interpolations)
Since arguments1.length === 0, it does eval(path.toString()) which would execute the Html.css call.
Then it finds if this is inside an object expression. If it's assigned to a variable like above, it might not be inside an object, so it would go to the else branch (lines 82-99) and create DOM manipulation statements.
If it was inside a style object like:
<div style={Html.css`
  color: red;
`} />
Then it would find the ObjectExpression (the style object), check if it has a style property (it wouldn't initially), and add one with CSS variable properties.
The uniqueId counter ensures each extraction gets unique variable names like --kentacss0-0, --kentacss0-1, etc.
The units array captures any units from the CSS (like px, em, etc.) to properly reconstruct the values.
Finally, it writes all extracted CSS to out.css.
This plugin essentially converts emotion/css JS calls into CSS variable-based styling that gets extracted to a CSS file, enabling separation of concerns while maintaining the ability to use JS logic in CSS definitions.
Let me now formulate a detailed step-by-step explanation.
Step-by-Step Explanation of babel.transform.extractStyles.js
This file is a Babel plugin that extracts CSS-in-JS (using @emotion/css via Html.css) into standalone CSS files while replacing the original calls with CSS variable references.
1. Initialization (Lines 1-6)
- Requires @emotion/css (as ecss) and Node's fs module
- Initializes a counter unique = 0 for generating unique identifiers
- Exports a function that receives the Babel instance and returns a plugin configuration
2. Plugin Structure (Lines 7-16)
- Returns a Babel plugin named "extract-css"
- Defines a visitor object that targets CallExpression nodes (function calls)
3. Call Detection (Lines 11-12)
- Checks if the call matches: Html.css(...)
  - path.node.callee.type === 'MemberExpression' (object.property syntax)
  - path.node.callee.property.name === 'css'
  - path.node.callee.object.name == 'Html'
4. Setup Variables (Lines 13-17)
When a matching call is found:
- Initializes arrays: content (for extracted CSS), originalCode (template values), units (CSS units)
- Increments and stores uniqueId for unique variable naming
5. Mock Html Object (Lines 18-34)
- Creates a temporary Html object with a css method
- This method processes template literals using @emotion/css:
  - If there are interpolations (b.length), it:
    - Splits the template into parts
    - Extracts CSS units (like px, em) from values
    - Processes with ecss.css
  - Otherwise, directly processes with ecss.css
6. Template Processing (Lines 37-47)
- Handles two cases based on whether the template has interpolations:
  - No interpolations (line 40): Directly evaluates path.toString() (the original Html.css call)
  - With interpolations (lines 42-46):
    - Replaces each interpolation value with a CSS variable placeholder string (e.g., var(--kentacss0-0))
    - Stores original values in originalCode
    - Evaluates the modified call to get the CSS string
7. Parent Object Detection (Lines 49-52)
- Finds the nearest parent ObjectExpression (likely a style object in JSX)
8. Style Property Handling (Lines 54-81)
Determines how to store the extracted CSS values:
Case A: Object has existing style property (Lines 57-64)
- Finds the existing style property
- For each original value:
  - Adds a CSS variable property (e.g., --kentacss0-0: "16px")
  - Preserves units separately if present
Case B: Object has no style property (Lines 65-80)
- Adds a new style property containing an object
- Each original value becomes a CSS variable property in this object
Case C: Not inside an object (Lines 82-99)
- Finds a parent with a body property (likely the module scope)
- For each value, creates a DOM manipulation statement that:
  - Immediately sets document.body.style.setProperty(name, value)
  - Also sets it on DOMContentLoaded (for safety)
  - Uses template literals to generate the code
9. CSS Extraction & Replacement (Lines 104-111)
- Collects all CSS inserted by emotion from ecss.cache.inserted
- Replaces the original Html.css(...) call with just the resulting CSS string (line 108)
- Writes all collected CSS to out.css file (line 111)
Key Transformation Examples
Input:
const style = Html.css`
  color: blue;
  margin: ${16}px;
`;
After Transformation:
1. Extracts CSS: color: blue; margin: 16px;
2. Generates unique variables: --kentacss0-0, --kentacss0-1
3. If in style object:
      const style = {
     style: {
       "--kentacss0-0": "blue",
       "--kentacss0-1": t.TemplateLiteral(["", "px"], [16])
     }
   }
   4. Writes to out.css: color: blue; margin: 16px;
5. Replaces call with: "color: blue; margin: 16px;" (though this seems unused)
If not in object:
- Creates DOM statements:
    ((body, name, value) => {
    if (body) body.style.setProperty(name, value);
    document.addEventListener('DOMContentLoaded', () => document.body.style.setProperty(name, value));
  })(document.body, "--kentacss0-0", "blue");
  ((body, name, value) => {
    if (body) body.style.setProperty(name, value);
    document.addEventListener('DOMContentLoaded', () => document.body.style.setProperty(name, value));
  })(document.body, "--kentacss0-1", t.TemplateLiteral(["", "px"], [16]));
  
Purpose
This plugin enables:
1. Extraction of CSS-in-JS to actual CSS files for better performance/caching
2. Conversion of dynamic values to CSS variables for runtime themeability
3. Preservation of original styling behavior while separating CSS concerns
4. Compatibility with build tools that optimize standalone CSS files
The approach uses CSS variables as a bridge between JavaScript logic and static CSS, allowing dynamic values to be updated at runtime while keeping the core CSS extractable.
