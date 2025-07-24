Overseer is a minimalistic tool that helps managing personal statistics, projects, tasks, journals and diaries and so on. It stays clear of any proprietary standards, closed-source tools and third-party hosting services. Instead it uses open human-readable protocol for the data it manages, and it self hosts the data locally with possibility of sharing.

Here are some examples of data this software is capable of managing: 

1) Personal tasks:
- Displaying a list of active tasks, sorted by priority
- Filtering active tasks by tags, projects, priority, age etc.
- Adding/Deleting a task at any moment
- Automatically activating a taks by calendar
- Automatically spawning a reccuring task, e.g. daily, weekly, etc
- Collecting and displaying some statistics about the tasks, e.g. 
- Easy to access button to indicate completion of a task
- Tasks can be parametric, e.g. exercise for N minutes, with ability to provide N when the task is finished 

2) Shopping lists:
- taking care of several parallel shopping lists for different shops
- easy to access checkbox to indicate item is bought
- some integration with personal tasks

3) Personal statistics:
- Storing data on personal progress, e.g. weight, exercise, sleep, skill training such as musical instruments etc
- some integration with tasks, e.g. when a parametric daily task "play midi drums" is complete with a score provided, the score can automatically go into personal progress statistics
- displaying statistics in a neat way, with graphs and tables

4) Personal projects:
- Storing notes, ideas, links, tables related to personal projects
- Providing basic bugtracker capabilities, similar to personal tasks but project-specific
- Integration with tasks, for example, when a bug is closed, a daily task "Work on project X for N storypoins" can update automatically
- Displaying statistics over project progress

IMPORTANT: Overseer is NOT supposed to provide out-of-the-box functionality for all of the described use cases; instead it's a minimalistic framework that can be used to make all of it possible: it should provide:
- a human-readable language (probably XML based or similar) to describe the relevant logic;
- a way/a convention to store and track the data
- software that displays the data according to the language, in a neat and understandable way

SOME POSSIBLE IMPLEMENTATION IDEAS:
- for storage and tracking, can git work? it provides self-hosting capacity and can easily be used for remote storage and sharing between devices. Of course this is a little outside of its intended use, and commiting every single change, such as one task being completed, can create very large repository history, so instead maybe just amend changed during one day, and then keep a commit per day? can this work with multiple devices, e.g. when I update a shopping list on my pc, can I see the update on my phone without having to refresh manually?

- for displaying, we need a cross platform and open source tool. An obvious choice is probably some web browser platform, but what other options do we have? Can we use Godot?

- for the language, I'm thinkig the generic capabilities it will provide should be similar to Excel, except instead of tables, the fields should have a hierarchical organization, thus XML-based or similar

BRAINSTORM ITERATION 1:
Q: How technical do you expect your users to be?
A: Users will definitely be comfortable editing XML/YAML/JSON-like files directly to define the logic of their data tables/lists/pages, but for editing separate data points there should definitely be a UI option, not because users can be non-technical but just to speed up the workflow.

Q: Have you considered YAML or TOML as alternatives to XML?
A: I agree, these are probably better or we can design our own markup language when we have a well defined feature set. XML was just an example.

Q: Git-based Storage has some challenges. Are you thinking of building a simple sync service, or expecting users to handle this manually?
A: Ideally users should be able to receive updates automatically, without having to refresh anything.

Q: Have you considered alternatives like a simple file-based approach?
A: We'll have to figure out backups and how to manage changes to data, but you're probably right, this is better. Let's return to this question after we determine the exact feature set.

Q: Display/UI Platform: "web browser platform" vs "Godot" - What's your priority: ease of development, native feel, or something else?
A: Ease of development is definitely the main priority

Scope Questions
Q: For the Excel-like capabilities: are you thinking of formula evaluation, or more like structured data entry with validation?
A: Yes, I was talking about formula evaluation and ability to dynamically reference other fields. Just like we can do [A1] in excel to address a cell, in Overseer we'll be able to do for example [tasks.5.description] to reference the description of the 5th task.

Q: How do you envision users initially setting up their data schemas? Through the UI or by editing config files?
A: The schemas will be set up by editing config files, same with the style options, and the software will only provide visualization + ability to edit specific fields and add new datapoints where applicable + manage storage and sync

Thoughts on tech suggestions
Suggestion 1. Consider a hybrid approach: JSON/YAML for data, with a simple schema definition language for structure
Comment: I actually would prefer to have everything in one place: structure, data, and style. This is not a good solution for scaling, but for minimalistic personal use I think this will be easier to manage.

Suggestion 2. For sync: Maybe look into CRDTs
Comment: I have to read more about this, on the first glance it does look like a good fit for us

Suggestion 3. For display: A web-based approach with offline capabilities (PWA) might give you the best of both worlds
Comment: Yes, as long as we'll be able to perform some graphical operations such as build and display charts from the data, I think this should work.

Some additional information
Example A1.
This is an example of what the code might look like using our finalized Overseer syntax:

```overseer
tab Tasks {
  div (hidden=true) { // unnamed div does not affect hierarchy, just to hide contents
    div Task (background=$(Priority>20?Red:White)) { // template for all tasks
      string Header = ""
      text Description = "" // text field allows markdown
      int Priority = 0
      date Created = today()
      date Due = ""
    }
  }
  
  list Data (entry=../Task) { // list will provide CRUD capabilities in the UI, as well as sorting, filtering etc
    div task1 (base=../../Task) {
      Header = "Test task" // other fields are copied from template
    }
  }
}
```


BRAINSTORM ITERATION 2:

Q: Formula syntax: In your Excel-like references [tasks.5.description], how would you handle dynamic references, aggregations, etc. Would you support something like [tasks.where(Priority>10).count()]?
A: Yes, we absolutely need this functionality. I think it should be implemented as a part of list type. Let's rewrite the [task.5.description] with our finalized syntax: it will become Task/Data/5/Description. Now we can call the list methods like this: $(Task/Data.where(Priority>10).count())
To get the one with the highest priority we can do $(Task/Data.sort_decreasing(Priority)[0]) or $(Task/Data.find_max(Priority))

Q: Template inheritance: In your example, can templates inherit from other templates?
A: Yes, they can, but I think this won't be a big deal because templates can just be a syntactic sugar for default initialization

Q: Data types: You show <string>, <text>, <int>, <date> - are you planning other types like: <enum>, <reference>, <file>
A: Yes, <enum>s are definitely needed, the rest we'll add later on little by little

Q: File organization: Since you want "everything in one place," how would you handle a file getting very large (hundreds of tasks)?
A: I only meant that I don't want to split logic, data and styles/themes in order to keep the language minimalistic; It makes perfect sense to split the files though, we can just combine the file system with our hierarchy: for example to access the tasks from another file in the same directory we'd prefix an extra ../ to the path: ../tasks.os/Tasks/Data
We'll leave it to the user to design their logic so that the files won't get too big, for example old finished tasks can be removed from the active list automatically and stored in separate files grouped by month

Q: Real-time updates: For automatic sync without manual refresh, you'd need
File watching on the local system
Some kind of notification system between devices
Conflict resolution when simultaneous edits occur
Have you thought about using something like a simple HTTP server that watches the file and pushes updates via WebSockets?
A: We only want to automatically push the updates that are made through the UI, so we don't need to run any software constantly to watch the files for that. The updates that the user made to the logic through direct file modification we'll let the user push manually. To receive updates automatically of course your logic still applies, I'm not sure what is the best way to do this, especially on mobile, and I'd like our UI client to work on mobile (e.g. to read and check shopping lists and to update outdoor related tasks)
For conflict resolution we also only have to care about the changes made through UI, we can let users deal with the conflicts in the logical part manually. And I think we can guarantee that the global hierarchy can't be changed through UI, only field values can be changed and elements can be added/removed from lists etc

Q: Chart generation: For your statistics displays, are you thinking simple charts (line, bar, pie) or more complex visualizations? What about integration with existing libraries like Chart.js or D3?
A: Simple charts are absolutely a must, and it would be nice for them to have some interactivity, i.e. for a user to be able to zoom in/out on a line chart, hover the mouse to see specific values at a given point etc. And ideally we should be able to stylize them through the language. I have a very limited knowledge of web-related libraries that can help with that, but I'm willing to try them out.

Workflow Questions
Q: Initial setup: When a user first runs Overseer, what would they start with?
A: For now let's just start with a completely empty directory, and later on we'll think on the best way to provide predefined patterns and templates

Q: Validation: How would you handle invalid formulas/type mismatches/circular references?
A: For now let's assume that our users are very tech savvy and their code will be perfect. I believe we'll be able to add detection for invalid formulas and type mismatches at any moment regardless of our architectural choices, so the only real potential problem is circular references. We'll keep that in mind when designing the rest of the system.

Q: Computed fields: Should some fields be entirely computed (never stored, always calculated)? Like "DaysUntilDue" = Due - Today?
A: Yes, but the way it should work is by using a formula inside of the field value:
```overseer
int DaysUntilDue = $(../Due - today())
```
And the way to avoid it being stored for each task is to simply keep it in the template, without redefining this field for specific tasks

Q: Actions/triggers: Beyond display, do you want the ability to define actions? Like "when task is marked complete, increment statistics.tasksCompleted"?
A: Excellent question, yes, we absolutely need this functionality. It's okay if these actions can only happen when the client is running.

Q: Import/Export: How important is compatibility with existing tools (CSV, JSON export, etc.)?
A: Let's not make this a part of the core functionality, we'll be able to add some simple additional scripts to cover some of this later on, but I agree this can be useful.

ADDITIONAL INFORMATION:
One of the reasons for keeping the data, the styling, and the logic in the same hierarchy is for that hierarchy to work simultaneously for both data access and visualization:
```overseer
div DiaryContent (horizontal-size=30%, border=Right) {
  // content
}
text DiaryPage (background=LightGray) {
  // markdown text here gets placed in the right 70% of the area, kind of like HTML divs
}
```

BRAINSTORM ITERATION 3:

Observation: The design reduces cognitive overhead.
A: This is a very good observation, minimizing cognitive overhead is precisely the main goal so let's consider it a part of our philosophy.

Q: Trigger syntax: How would you define these?
A: Let's add them inside of the relevant hierarchy like this:
```overseer
div Task {
  // ... other Task fields
  trigger (condition=$(../DaysTillDue < 2)) {
    action Set (target=../Priority) = $(../BasePriority + 10)
  }
  
  checkbox Completed {
    action Add (target=../../Statistics/TasksCompleted) = 1
  }
  
  button Bump "Bump priority" {
    action Set (target=../Priority) = $(../Priority + 1)
  }
}
```
this way we can add actions not only to triggers, but also to checkboxes, buttons, etc

Q: Action scope: Should actions be able to:
Modify other files? (e.g., completing a task updates a separate statistics file)
Create new items? (e.g., completing a project task spawns a new daily task)
Send notifications? (e.g., email when high-priority task is overdue)
A: Let's limit other file access just to the project hierarchy, i.e. let them modify anything in the hierarchy even if it's not in the local file, but for now let's skip a more generic interface to edit any file (but maybe we'll add it later on)
Actions should definitely be able to create new items, but as for notifications let's not add that for now.

Q: Layout types: Do we need grid layouts for complex arrangements?
A: right now we have list type that will provide layout options for tables. User should be able to select vertical or horizontal layout, and there should be a way to make it align the corresponding fields inside of its elements so that they would form a grid. For now this should be the only grid functionality we need.

Q: What about responsive design for mobile?
A: we can keep mobile functionality to the bare minimum, as long as buttons and checkboxed can be accessed through touch screen it's fine

Q: Tabs, accordions, modals for complex UIs?
A: Yes we've already mentioned tabs, we definitely need them, the functionality can be as simple as "load everything, show the selected tab, hide the others". And in terms of layout again as long as we can select between vertical and horizontal tabs it's fine.
For accordions and drop-down lists let's simply add a way to hover a div on top of everything, then we can hide it in code and show when the relevant button is pressed. Then we can implement the rest as templates in our language.

Q: How would you define form inputs for adding new items?
A: All string and text fields should be mutable by default unless opposite is specified in the header. In the UI when user double clicks on a string or text, it switches to edit mode, displaying unformatted markdown text and/or formulas, same as in Excel. With this we won't need any additional node types for form inputs.
For adding new items to a list we can just append a default template to the list and let the user modify the fields.

Q: Global variables: You mentioned $Today - what other globals might be useful?
A: For our prototype build let's just have today() function, what's more important is to provide the means to control when these global variables are evaluated, e.g. for the Task it should be evaluated when the task is added, so that it would turn into a constant date for a specific task. Also it's probably better to just have it as a today() function and remove the concept of global variables completely

Q: How are we going to deal with cross-file dependencies?
A: At this stage let's assume that all code will be well-formed, with no broken references and circular dependencies, just like we did for formulas

Observation: For shopping lists on mobile, you'll need:
Touch-friendly UI elements - Yes, let's keep this in mind
Offline capability when network is poor - Let's not bother with optimization for our MVP
Efficient syncing to preserve battery/data - same here

Q: Performance with formulas - as lists grow, Task/Data.where(Priority>10).count() could get expensive. Are you thinking of caching computed values?
A: Yes, I think we should at least leave room to add caching later on. For example our software can create a hidden directory same way git or vscode do and store cached values for fields there.

Q: MVP definition: What would be the absolute minimum viable version? Just static display of one file? Adding CRUD operations?
A: Exactly, let's start with static display of one file, supporting basic hierarchy, a few basic node types, simple style/layout options.

Suggestion: I'm seeing potential development phases like:
Phase 1: File parsing + static display
Phase 2: Basic CRUD operations
Phase 3: Formula evaluation
Phase 4: Actions/triggers
Phase 5: Multi-device sync
Commentary: Yes, this looks straightforward.

Tech stack decisions: Because of lack of expertise in Web, I don't feel competent to select any of these options at the moment, I'll need a more detailed explanation on how exactly our software will work in each of these stacks, and I'll have to read some basic information on these technologies

BRAINSTORM ITERATION 4:

Syntax finalization:
- Formula syntax: Use $() instead of [] to avoid collision with array indexing
- Node syntax: type NodeName (param=value, param=value) { ... } where NodeName is optional
- Parameters use parentheses for clean separation from node content
- Clear distinction between formulas $(), array indexing [0], and parameters ()

Status: Syntax is now solidified and ready for implementation documentation.

BRAINSTORM ITERATION 5:

Tech stack decision: Tauri (Rust + Web frontend)
- Desktop: Tauri native app with web UI
- Mobile: Tauri Mobile for Android (and later iOS)
- Frontend: HTML/CSS/JavaScript with Chart.js for visualization
- Backend: Rust for file parsing, formula evaluation, and business logic
- Cross-platform single codebase
- Native performance with web UI flexibility
- Great opportunity to learn Rust coming from C++/C background

Next steps: Set up basic Tauri project structure and begin implementation