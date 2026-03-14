First, this is a Babel plugin that transform JavaScript code.

The purpose of this transform is to extract Html.css tagged template strings and return unique css class names to allow for css in javascript;

The original transform used eval to get the part of the template and the substitutions, In fact there' s 2 pattern and 2 things to do when collecting:
pattern 1: when there's no substitution: easy path, just call emotion
pattern 2: when there's substitution to be made (there are template literal arguments) it would generate a unique css class --kenta-css-${UNIQUE_ID}-${SUBSTITUTE_INDEX} and interpolate the string replacing the real substitution with the css var: `var(--kenta-css-${UNIQUE_ID}-${SUBSTITUTE_INDEX})` only then would call emotion on it, but for this path because of the eval also the following would happen:
  when concatenating the template string as per above for each substitution we would look into the next token and if it start with ';' we would assume it's a unitless value (and for convenience put everything in unit in that case), otherwise we would save whatever is on the left side as unit and put the rest in res.

Then we need to add back the substitution so that will populate the css variables we add, so to do that we do in 2 ways:
- check if the first parent object expression has a style attribute
- if not we'll set it in the script body, both immediately and in domcontentload event in case document.body is not available.

The collect classes and css will finally be written into a .css file.