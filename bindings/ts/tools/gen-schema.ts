import * as TJS from "typescript-json-schema";
import {resolve} from "path";
import {writeFileSync} from "fs";

const basePath = "./src";
const program = TJS.getProgramFromFiles(
    [resolve("./src/task_maker.ts")],
    basePath
);

const exportSchema = (typeName: string) => {
    const schema = TJS.generateSchema(program, typeName);
    const json = JSON.stringify(schema);
    writeFileSync(`./schema/${typeName}.schema.json`, json);
};

exportSchema("UIMessage");
exportSchema("TaskInfo");