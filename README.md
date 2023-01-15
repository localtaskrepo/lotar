# Idea

Local Task Repo -  LoTaR

A task and project manager that lives in your repository.

__Current State:__
* Quickjs runs with the web server and can run compiled react code from parcel.
* We can't compile an app because the web server causes an error (```const uint8_t qjsc_index[1120]```)
* The docker image is all set up and fails at the same point
* At this point I think the issues is more likely on qiuckjs' side then the webserver, but I'm not sure.
* Options now are:
  * Wait for the developers to come back (I think they're Ukrainian, so not until the war is over, if ever)
  * Try to fix the existing webserver myself
  * Find another way to run a web server in quickjs

_Features:_
* Command line interface to manage tasks and projects
  * Should work in any subdirectory and find the info automatically (doesn't need to be in root)
* Plugins for popular IDEs (intelliJ first) that interact with the data
* All data is stored locally, possibly using git as a backend (but likely not necessary)
  * The system should be git aware (e.g. if task has not been committed it should be able to show)
* The Folder structure should be such that each task is a file and each group, project, etc. is a folder
* Think about whether this allows some way of supporting tags instead (But I don't think that's easily done while being straight forward to understand and implement
* It should be able to provide it's own web interface instead of relying on CLI and IDE plugins
* Should also provide it's own web server that can be queried for status (which can be used by the IDE plugins))
* It should be able to scan existing files for patterns and then link files and references to tasks
  * It should also be able to modify the files to add links to tasks, but only when enabled
  * This function should be available as daemon and as a command line tool (as should pretty much all functions)
  * It should allow for some control on what the metadata on an issues is when looking for text in files. Maybe @task is one variation, @bug is a variation of @task(type=bug), or something like that
* The structure should be so that groups can be moved around easily because any metadata is partition by groups.
* Allow to set the .tasks path via argument, environment variable, or package.json/localtr.config
  * Can look for other package formats that may be user, e.g. maven or gradle files.
* The system should be highly configurable with sensible defaults out of the box
  * Custom states, multiple finish states, custom fields
  * dynamic variables that can be used these fields (e.g. now(), me(), etc.)
* This is assuming a single user/developer on a project so no user management, assignments or anything like that are needed
  * Users can still add people fields through custom fields, e.g. for customers of not for this.
* triggers for when tasks are created, finished, etc.*
* Transition graphs ala JIRA? (stretch goal))
* Project Structure:
  * cli -> reads input from file, env, args, folder structure, and makes it available to the rest of the system
    * Should maybe only load data from the current CONTEXT (which needs to be passed on when calling functions)
  * scanner -> Scans files for patterns and links them to tasks
  * api -> provides a REST api for the data
  * data -> provides the data from the folder structure, indices, search, etc.
  * tickets -> provides a ticket parser and objects
  * web -> provides a web interface for the data
  * hooks -> executes hooks when tasks are created, finished, etc. (should try to run these in seperate processes to protect the main process from crashing)
* It should be able to process modified files and regenerate as much of the indices and metadata as possible.
* Allow data hooks for different data types (e.g. connect to a people directory via hook that can be used for autocomplete)

# Tasks File structure
We're using json everywhere because that's built in, but yaml would be preferred. Also we will need some form of custom
parser for the md files to consume the metadata at the end. an ini parser might do it, but it might be easier just to
write it ourselves. That's not the case for yaml, however maybe a simple ini format like that might be enough. TBD

```shell
.tasks
  metadata.json  # Global metadata and settings
  hooks.json     # Global hooks, defining what to run in each state
  /project1
    metadata.json  # Project metadata
    index.json  # Group indices
    /group1
      metadata.json  # Group metadata
      name1.md
      name2.md
    /group2
      metadata.json
      name3.md
      name4.md
  /project2
    metadata.json
    index.json
    /group3
      metadata.json
      index.json
      name5.md
      name6.md
    /group4
      metadata.json
      index.json
      name7.md
      name8.md
```

# Tasks File Format
```yaml
Title: My Task
Subtitle: (optional) Subtitle
Description: (optional) Description
ID: 1234
Status: TODO
Priority: 1
Due: 2019-01-01
Created: 2019-01-01
Modified: 2019-01-01
Tags: [tag1, tag2
Custom-X: value
```

# Transition Graph File Format
```yaml
"TODO": ["IN_PROGRESS", "VERIFY"],
"IN_PROGRESS": ["VERIFY", "BLOCKED"],
"VERIFY": ["DONE", "BLOCKED"],
"BLOCKED": ["TODO", "IN_PROGRESS"],
"DONE": [] // Actually not needed, but here for completeness
```

# Database index.js file

```json
{
  "id2file": { // Basically a primary key index
    "id21345": "project1/group1/name1.md"
  },
  "tag2id": { // Lookup table for indices
    "tag1": ["id21345"],
    "tag2": ["id21345"]
  },
  "file2file": { // Lookup table for file relationships
    "project1/group1/name1.md": "project1/group1/name1.md"
  }
}
```

# CLI command pattern

_Examples:_
Interactive shell with wizard to create task
```localtr add -i "Task title"```

Single command mode
```localtr add "Task title" -p "Project 1" -g "Group 1" -p 1 -d 2019-01-01 -t tag1 -t tag2,tag3```

Update a task status using the task id
```localtr status "in progress" task1```

Update a task status using the task title
```localtr status "in progress" "Task title"```

Query tasks
```localtr search -p "Project 1" -g "Group 1" -s "in progress" -t tag1 -t tag2,tag3```

# Reference Links

* https://www.npmjs.com/package/git-state

_Command line compiler and interface_
* https://bellard.org/quickjs/quickjs.html
* https://github.com/QuickJS-Web-project/quickwebserver
* May have to implement git status yourself
* Engine based on QuickJS in case the raw thing is too simple: https://github.com/saghul/txiki.js

_IntelliJ Plugin Development: (https://www.jetbrains.org/intellij/sdk/docs/basics/getting_started.html)_
* https://github.com/emaayan/RTCTasks/tree/master/src/main/java/org/rtctasks
* https://github.com/mayankmkh/linear-app-plugin
* https://github.com/norrs/launchpad-intellij-tasks-provider/tree/master/src/no/norrs/launchpad/tasks
* https://github.com/JetBrains/youtrack-idea-plugin
