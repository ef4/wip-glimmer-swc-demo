import chai from "chai";
import { codeEquality } from "code-equality-assertions/chai";
import { Preprocessor } from "content-tag";

chai.use(codeEquality);

const { expect } = chai;

const p = new Preprocessor();

describe(`process`, function () {
  it("works for a basic example", function () {
    let output = p.process("<template>Hi</template>");

    expect(output).to
      .equalCode(`import { template } from "@ember/template-compiler";
  export default template(\`Hi\`, {
      eval () {
          return eval(arguments[0]);
      }
  });`);
  });

  it("escapes backticks", function () {
    let input = `
    class Foo extends Component {
      greeting = 'Hello';

      <template>{{this.greeting}}, \`lifeform\`!</template>
    }
  `;

    let output = p.process(input);

    expect(output).to.equalCode(
      `import { template } from "@ember/template-compiler";
     class Foo extends Component {
         greeting = 'Hello';
         static{
             template(\`{{this.greeting}}, \\\`lifeform\\\`!\`, {
                 component: this,
                 eval () {
                     return eval(arguments[0]);
                 }
             });
         }
     };`
    );
  });

  it("Emits parse errors with anonymous file", function () {
    expect(function () {
      p.process(`const thing = "face";
  <template>Hi`);
    }).to.throw(`Parse Error at <anon>:2:15: 2:15`);
  });

  it("Emits parse errors with real file", function () {
    expect(function () {
      p.process(
        `const thing = "face";
  <template>Hi`,
        { filename: "path/to/my/component.gjs" }
      );
    }).to.throw(`Parse Error at path/to/my/component.gjs:2:15: 2:15`);
  });

  it("Offers source_code snippet on parse errors", function () {
    let parseError;
    try {
      p.process(`class {`);
    } catch (err) {
      parseError = err;
    }
    expect(parseError)
      .to.have.property("source_code")
      .matches(/Expected ident.*class \{/s);
  });

  it("Offers source_code_color snippet on parse errors", function () {
    let parseError;
    try {
      p.process(`class {`);
    } catch (err) {
      parseError = err;
    }
    // eslint-disable-next-line no-control-regex
    expect(parseError)
      .to.have.property("source_code_color")
      .matches(/Expected ident.*[\u001b].*class \{/s);
  });

  it("Provides inline source maps if inline_source_map option is set to true", function () {
    let output = p.process(`<template>Hi</template>`, { inline_source_map: true });

    expect(output).to.match(
      /sourceMappingURL=data:application\/json;base64,eyJ2ZXJzaW9uIjozLCJzb3VyY2VzIjpbIjxhbm9uPiJdLCJzb3VyY2VzQ29udGVudCI6WyI8dGVtcGxhdGU-SGk8L3RlbXBsYXRlPiJdLCJuYW1lcyI6W10sIm1hcHBpbmdzIjoiO0FBQUEsZUFBQSxTQUFVLENBQUEsRUFBRSxDQUFBLEVBQUE7SUFBQTtRQUFBLE9BQUEsS0FBQSxTQUFBLENBQUEsRUFBVztJQUFEO0FBQUEsR0FBQyJ9/
    );
  });
});
