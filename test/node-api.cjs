const { Preprocessor } = require("content-tag");
const chai = require("chai");
const { codeEquality } = require("code-equality-assertions/chai");

chai.use(codeEquality);

const { expect } = chai;

const p = new Preprocessor();

describe("parse", function () {
  it("basic example", function () {
    let output = p.parse("<template>Hello!</template>");

    expect(output).to.eql([
      {
        type: "expression",
        tagName: "template",
        contents: "Hello!",
        range: {
          start: 0,
          end: 27,
        },
        contentRange: {
          start: 10,
          end: 16,
        },
        startRange: {
          end: 10,
          start: 0,
        },
        endRange: {
          start: 16,
          end: 27,
        },
      },
    ]);
  });

  it("multi-byte characters", function () {
    let contentLength = "안녕하세요 세계".length;
    let openLength = "<template>".length;
    let closeLength = "</template>".length;
    let output = p.parse("<template>안녕하세요 세계</template>");

    expect(output).to.eql([
      {
        type: "expression",
        tagName: "template",
        contents: "안녕하세요 세계",
        range: {
          start: 0,
          end: openLength + contentLength + closeLength,
        },
        contentRange: {
          start: openLength,
          end: openLength + contentLength,
        },
        startRange: {
          end: openLength,
          start: 0,
        },
        endRange: {
          start: openLength + contentLength - 1,
          end: openLength + contentLength + closeLength,
        },
      },
    ]);
  });

  it("expression position", function () {
    let output = p.parse("const tpl = <template>Hello!</template>");

    expect(output).to.eql([
      {
        type: "expression",
        tagName: "template",
        contents: "Hello!",
        range: {
          start: 12,
          end: 39,
        },
        contentRange: {
          start: 22,
          end: 28,
        },
        startRange: {
          start: 12,
          end: 22,
        },
        endRange: {
          start: 28,
          end: 39,
        },
      },
    ]);
  });

  it("inside class body", function () {
    let output = p.parse(`
      class A {
        <template>Hello!</template>
      }
    `);

    expect(output).to.eql([
      {
        type: "class-member",
        tagName: "template",
        contents: "Hello!",
        range: {
          start: 25,
          end: 52,
        },
        contentRange: {
          start: 35,
          end: 41,
        },
        startRange: {
          start: 25,
          end: 35,
        },
        endRange: {
          start: 41,
          end: 52,
        },
      },
    ]);
  });

  it("preceded by a slash character", function () {
    // What is this testing?
    // Would a better test be:
    // `const divide = 1 / <template>Hello!</template>;`
    let output = p.parse(`
      const divide = () => 4 / 2;
      <template>Hello!</template>
    `);

    expect(output).to.eql([
      {
        type: "expression",
        tagName: "template",
        contents: "Hello!",
        range: {
          start: 41,
          end: 68,
        },
        contentRange: {
          start: 51,
          end: 57,
        },
        startRange: {
          start: 41,
          end: 51,
        },
        endRange: {
          start: 57,
          end: 68,
        },
      },
    ]);
  });

  it("/<template>/ inside a regexp", function () {
    let output = p.parse(`
      const myregex = /<template>/;
      <template>Hello!</template>
    `);

    expect(output).to.eql([
      {
        type: "expression",
        tagName: "template",
        contents: "Hello!",
        range: {
          start: 43,
          end: 70,
        },
        contentRange: {
          start: 53,
          end: 59,
        },
        startRange: {
          start: 43,
          end: 53,
        },
        endRange: {
          start: 59,
          end: 70,
        },
      },
    ]);
  });

  it("no match", function () {
    let output = p.parse("console.log('Hello world');");

    expect(output).to.eql([]);
  });

  it("Emits parse errors", function () {
    expect(function () {
      p.process(
        `const thing = "face";
  <template>Hi`,
        "path/to/my/component.gjs"
      );
    }).to.throw(`Parse Error at path/to/my/component.gjs:2:15: 2:15`);
  });
});

describe("process", function () {
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
       let Foo = class Foo extends Component {
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
        "path/to/my/component.gjs"
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
});
