# bundled_npm_through_system_node_shim

## `vpt write-file .node-version 20.18.0`


## `vp env exec --node 22.18.0 node assert-system-node-shim.cjs setup`


## `vp env off node`


## `vp env on npm`


## `PATH=${VP_HOME}/bin${PATH_SEPARATOR}${workspace}/system-shims${PATH_SEPARATOR}/usr/bin${PATH_SEPARATOR}/bin node assert-system-node-shim.cjs`

Bundled npm/npx and their children use Node 22 behind the system shim despite the project's Node 20 pin

```
Bundled npm/npx use the runtime behind the system Node shim
```
