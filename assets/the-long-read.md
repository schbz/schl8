# The Long Read
_A deliberately lengthy sample document for testing Schl8's crawl mode._

This file exists to be scrolled. It is long enough to take a while at a
comfortable reading speed, and varied enough to exercise headings, lists,
quotes, code blocks, tables and long paragraphs as they pass the reading
line. Nothing in it is secret; it is a fixture, not a note.

Open it, then start the crawl with **Cmd+Shift+R**, or from the
**View** menu, **Crawl (auto-scroll)**. While it runs:

| Key        | Does                          |
|------------|-------------------------------|
| `Space`    | Pause / resume                |
| `Up` `Down`| Faster / slower               |
| `+` `-`    | Bigger / smaller text         |
| `R`        | Reverse direction (also restarts a crawl parked at an end) |
| `Home` `End` | Jump to start / end         |
| `Esc`      | Leave crawl mode              |

Scrolling by hand works exactly as it normally does; the crawl steps
aside, then picks itself back up a couple of seconds after you stop,
continuing from wherever you left it. By default it turns around at
either end rather than stopping. Everything else is in Settings.

---

## Section 1
Encryption is a boundary, not a filing system; the two get confused more often than they should. Line length turns out to matter as much as speed. A column that runs the full width of a modern display forces the eye to travel a long way back at every line break, and while the text is also moving vertically that return sweep becomes genuinely difficult. Narrower is better here than it is for static text.

There is an argument that this is a toy. There is a better argument that reading is the operation a notes application performs most often and optimizes for least, and that anything making long-form review less effortful earns its place. Whether that is worth a mode of its own is a question best answered by using it on something long.

## Section 2
Reading at a fixed pace is a different act from scanning, and the difference shows up almost immediately. Speed matters more than it seems. Too slow and attention wanders forward, hunting for the next line before it arrives. Too fast and comprehension quietly degrades while the sense of progress stays high, which is the worse failure of the two because it does not announce itself.

Reversing direction is stranger than it sounds and more useful than expected. Something read three paragraphs ago becomes reachable without abandoning the mode, and the reader's place is preserved rather than lost to a scrollbar drag. Whether that is worth a mode of its own is a question best answered by using it on something long.

## Section 3
Every archive eventually contains something its author no longer remembers writing. When the words arrive on a schedule you stop deciding when to move, and that small surrender is most of the effect. The eye settles into a band near the middle of the screen and stays there. Lines enter from below, cross the reading line, and leave. What would have been a series of small decisions about scrolling becomes one decision made once, at the start.

Speed matters more than it seems. Too slow and attention wanders forward, hunting for the next line before it arrives. Too fast and comprehension quietly degrades while the sense of progress stays high, which is the worse failure of the two because it does not announce itself. None of this is novel; it is simply rare to find it applied to one's own notes.

## Section 4
The first thing a reader does with an unfamiliar file is look for its edges. Speed matters more than it seems. Too slow and attention wanders forward, hunting for the next line before it arrives. Too fast and comprehension quietly degrades while the sense of progress stays high, which is the worse failure of the two because it does not announce itself.

The edges of the window are where the illusion breaks. A line that is abruptly cut in half by the top of the screen reads as an error rather than as motion. Softening those edges costs nothing and buys a surprising amount of comfort over a long document. Whether that is worth a mode of its own is a question best answered by using it on something long.

### Settings that change the feel most

- Speed — the obvious one, and the one worth adjusting per document.
- Column width — less obvious, and arguably more important.
- Text size — interacts with speed, since bigger text crosses the screen faster in lines per second.
- Edge fade — pure comfort, no function.

## Section 5
The first thing a reader does with an unfamiliar file is look for its edges. When the words arrive on a schedule you stop deciding when to move, and that small surrender is most of the effect. The eye settles into a band near the middle of the screen and stays there. Lines enter from below, cross the reading line, and leave. What would have been a series of small decisions about scrolling becomes one decision made once, at the start.

Speed matters more than it seems. Too slow and attention wanders forward, hunting for the next line before it arrives. Too fast and comprehension quietly degrades while the sense of progress stays high, which is the worse failure of the two because it does not announce itself. The test, as always, is whether you forget the mechanism and remember the content.

## Section 6
The trouble with a long document is that it asks you to hold your place while your hands are busy elsewhere. There is an argument that this is a toy. There is a better argument that reading is the operation a notes application performs most often and optimizes for least, and that anything making long-form review less effortful earns its place.

When the words arrive on a schedule you stop deciding when to move, and that small surrender is most of the effect. The eye settles into a band near the middle of the screen and stays there. Lines enter from below, cross the reading line, and leave. What would have been a series of small decisions about scrolling becomes one decision made once, at the start. The test, as always, is whether you forget the mechanism and remember the content.

> A document you never reread is a document you never really wrote.

## Section 7
There is a particular kind of note that only reveals its shape on a second pass, when nothing is being edited. Pausing has to be instant and obvious. The moment a reader wants to stop, any delay feels like fighting the software. The same is true in reverse: resuming should not require finding a control, only pressing the key that stopped it.

There is an argument that this is a toy. There is a better argument that reading is the operation a notes application performs most often and optimizes for least, and that anything making long-form review less effortful earns its place. The test, as always, is whether you forget the mechanism and remember the content.

```rust
// Section 7: the motion is just arithmetic over a scroll offset.
let delta = speed * dt.clamp(0.0, 0.25);
offset += if direction_up { delta } else { -delta };
if offset >= max_scroll {
    if loop_at_end { offset = 0.0 } else { offset = max_scroll }
}
```

## Section 8
Reading at a fixed pace is a different act from scanning, and the difference shows up almost immediately. Pausing has to be instant and obvious. The moment a reader wants to stop, any delay feels like fighting the software. The same is true in reverse: resuming should not require finding a control, only pressing the key that stopped it.

Line length turns out to matter as much as speed. A column that runs the full width of a modern display forces the eye to travel a long way back at every line break, and while the text is also moving vertically that return sweep becomes genuinely difficult. Narrower is better here than it is for static text. Whether that is worth a mode of its own is a question best answered by using it on something long.

### Settings that change the feel most

- Speed — the obvious one, and the one worth adjusting per document.
- Column width — less obvious, and arguably more important.
- Text size — interacts with speed, since bigger text crosses the screen faster in lines per second.
- Edge fade — pure comfort, no function.

## Section 9
Every archive eventually contains something its author no longer remembers writing. Reversing direction is stranger than it sounds and more useful than expected. Something read three paragraphs ago becomes reachable without abandoning the mode, and the reader's place is preserved rather than lost to a scrollbar drag.

Speed matters more than it seems. Too slow and attention wanders forward, hunting for the next line before it arrives. Too fast and comprehension quietly degrades while the sense of progress stays high, which is the worse failure of the two because it does not announce itself. Whether that is worth a mode of its own is a question best answered by using it on something long.

#### A closer look (9)

When the words arrive on a schedule you stop deciding when to move, and that small surrender is most of the effect. The eye settles into a band near the middle of the screen and stays there. Lines enter from below, cross the reading line, and leave. What would have been a series of small decisions about scrolling becomes one decision made once, at the start.

## Section 10
Every archive eventually contains something its author no longer remembers writing. Documents of this length are exactly where paging breaks down. Thirty screenfuls of scrolling is thirty small interruptions, each one an opportunity to lose the thread or decide the reading can wait.

There is an argument that this is a toy. There is a better argument that reading is the operation a notes application performs most often and optimizes for least, and that anything making long-form review less effortful earns its place. It is a small feature with an unusually direct relationship to how the application actually gets used.

## Section 11
Notes accumulate faster than they are revisited, which is the whole reason revisiting needs to be pleasant. Documents of this length are exactly where paging breaks down. Thirty screenfuls of scrolling is thirty small interruptions, each one an opportunity to lose the thread or decide the reading can wait.

Reversing direction is stranger than it sounds and more useful than expected. Something read three paragraphs ago becomes reachable without abandoning the mode, and the reader's place is preserved rather than lost to a scrollbar drag. It is a small feature with an unusually direct relationship to how the application actually gets used.

## Section 12
Every archive eventually contains something its author no longer remembers writing. Line length turns out to matter as much as speed. A column that runs the full width of a modern display forces the eye to travel a long way back at every line break, and while the text is also moving vertically that return sweep becomes genuinely difficult. Narrower is better here than it is for static text.

The edges of the window are where the illusion breaks. A line that is abruptly cut in half by the top of the screen reads as an error rather than as motion. Softening those edges costs nothing and buys a surprising amount of comfort over a long document. Whether that is worth a mode of its own is a question best answered by using it on something long.

### Settings that change the feel most

- Speed — the obvious one, and the one worth adjusting per document.
- Column width — less obvious, and arguably more important.
- Text size — interacts with speed, since bigger text crosses the screen faster in lines per second.
- Edge fade — pure comfort, no function.

> The slowest part of reading is deciding to start.

## Section 13
Notes accumulate faster than they are revisited, which is the whole reason revisiting needs to be pleasant. Reversing direction is stranger than it sounds and more useful than expected. Something read three paragraphs ago becomes reachable without abandoning the mode, and the reader's place is preserved rather than lost to a scrollbar drag.

Documents of this length are exactly where paging breaks down. Thirty screenfuls of scrolling is thirty small interruptions, each one an opportunity to lose the thread or decide the reading can wait. It is a small feature with an unusually direct relationship to how the application actually gets used.

## Section 14
Reading at a fixed pace is a different act from scanning, and the difference shows up almost immediately. Speed matters more than it seems. Too slow and attention wanders forward, hunting for the next line before it arrives. Too fast and comprehension quietly degrades while the sense of progress stays high, which is the worse failure of the two because it does not announce itself.

There is an argument that this is a toy. There is a better argument that reading is the operation a notes application performs most often and optimizes for least, and that anything making long-form review less effortful earns its place. The test, as always, is whether you forget the mechanism and remember the content.

```rust
// Section 14: the motion is just arithmetic over a scroll offset.
let delta = speed * dt.clamp(0.0, 0.25);
offset += if direction_up { delta } else { -delta };
if offset >= max_scroll {
    if loop_at_end { offset = 0.0 } else { offset = max_scroll }
}
```

## Section 15
Encryption is a boundary, not a filing system; the two get confused more often than they should. Line length turns out to matter as much as speed. A column that runs the full width of a modern display forces the eye to travel a long way back at every line break, and while the text is also moving vertically that return sweep becomes genuinely difficult. Narrower is better here than it is for static text.

Documents of this length are exactly where paging breaks down. Thirty screenfuls of scrolling is thirty small interruptions, each one an opportunity to lose the thread or decide the reading can wait. None of this is novel; it is simply rare to find it applied to one's own notes.

## Section 16
The trouble with a long document is that it asks you to hold your place while your hands are busy elsewhere. Speed matters more than it seems. Too slow and attention wanders forward, hunting for the next line before it arrives. Too fast and comprehension quietly degrades while the sense of progress stays high, which is the worse failure of the two because it does not announce itself.

Reversing direction is stranger than it sounds and more useful than expected. Something read three paragraphs ago becomes reachable without abandoning the mode, and the reader's place is preserved rather than lost to a scrollbar drag. It is a small feature with an unusually direct relationship to how the application actually gets used.

### Settings that change the feel most

- Speed — the obvious one, and the one worth adjusting per document.
- Column width — less obvious, and arguably more important.
- Text size — interacts with speed, since bigger text crosses the screen faster in lines per second.
- Edge fade — pure comfort, no function.

## Section 17
Encryption is a boundary, not a filing system; the two get confused more often than they should. Documents of this length are exactly where paging breaks down. Thirty screenfuls of scrolling is thirty small interruptions, each one an opportunity to lose the thread or decide the reading can wait.

Documents of this length are exactly where paging breaks down. Thirty screenfuls of scrolling is thirty small interruptions, each one an opportunity to lose the thread or decide the reading can wait. Whether that is worth a mode of its own is a question best answered by using it on something long.

## Section 18
Reading at a fixed pace is a different act from scanning, and the difference shows up almost immediately. Pausing has to be instant and obvious. The moment a reader wants to stop, any delay feels like fighting the software. The same is true in reverse: resuming should not require finding a control, only pressing the key that stopped it.

Documents of this length are exactly where paging breaks down. Thirty screenfuls of scrolling is thirty small interruptions, each one an opportunity to lose the thread or decide the reading can wait. Whether that is worth a mode of its own is a question best answered by using it on something long.

> A document you never reread is a document you never really wrote.

#### A closer look (18)

Pausing has to be instant and obvious. The moment a reader wants to stop, any delay feels like fighting the software. The same is true in reverse: resuming should not require finding a control, only pressing the key that stopped it.

## Section 19
Notes accumulate faster than they are revisited, which is the whole reason revisiting needs to be pleasant. Pausing has to be instant and obvious. The moment a reader wants to stop, any delay feels like fighting the software. The same is true in reverse: resuming should not require finding a control, only pressing the key that stopped it.

There is an argument that this is a toy. There is a better argument that reading is the operation a notes application performs most often and optimizes for least, and that anything making long-form review less effortful earns its place. It is a small feature with an unusually direct relationship to how the application actually gets used.

## Section 20
The trouble with a long document is that it asks you to hold your place while your hands are busy elsewhere. Documents of this length are exactly where paging breaks down. Thirty screenfuls of scrolling is thirty small interruptions, each one an opportunity to lose the thread or decide the reading can wait.

Reversing direction is stranger than it sounds and more useful than expected. Something read three paragraphs ago becomes reachable without abandoning the mode, and the reader's place is preserved rather than lost to a scrollbar drag. The test, as always, is whether you forget the mechanism and remember the content.

### Settings that change the feel most

- Speed — the obvious one, and the one worth adjusting per document.
- Column width — less obvious, and arguably more important.
- Text size — interacts with speed, since bigger text crosses the screen faster in lines per second.
- Edge fade — pure comfort, no function.

## Section 21
Reading at a fixed pace is a different act from scanning, and the difference shows up almost immediately. Documents of this length are exactly where paging breaks down. Thirty screenfuls of scrolling is thirty small interruptions, each one an opportunity to lose the thread or decide the reading can wait.

When the words arrive on a schedule you stop deciding when to move, and that small surrender is most of the effect. The eye settles into a band near the middle of the screen and stays there. Lines enter from below, cross the reading line, and leave. What would have been a series of small decisions about scrolling becomes one decision made once, at the start. The test, as always, is whether you forget the mechanism and remember the content.

```rust
// Section 21: the motion is just arithmetic over a scroll offset.
let delta = speed * dt.clamp(0.0, 0.25);
offset += if direction_up { delta } else { -delta };
if offset >= max_scroll {
    if loop_at_end { offset = 0.0 } else { offset = max_scroll }
}
```

## Section 22
A page that moves on its own changes where your attention goes, and mostly it goes to the text. Line length turns out to matter as much as speed. A column that runs the full width of a modern display forces the eye to travel a long way back at every line break, and while the text is also moving vertically that return sweep becomes genuinely difficult. Narrower is better here than it is for static text.

The edges of the window are where the illusion breaks. A line that is abruptly cut in half by the top of the screen reads as an error rather than as motion. Softening those edges costs nothing and buys a surprising amount of comfort over a long document. None of this is novel; it is simply rare to find it applied to one's own notes.

## Section 23
The first thing a reader does with an unfamiliar file is look for its edges. Documents of this length are exactly where paging breaks down. Thirty screenfuls of scrolling is thirty small interruptions, each one an opportunity to lose the thread or decide the reading can wait.

Speed matters more than it seems. Too slow and attention wanders forward, hunting for the next line before it arrives. Too fast and comprehension quietly degrades while the sense of progress stays high, which is the worse failure of the two because it does not announce itself. The test, as always, is whether you forget the mechanism and remember the content.

## Section 24
Notes accumulate faster than they are revisited, which is the whole reason revisiting needs to be pleasant. There is an argument that this is a toy. There is a better argument that reading is the operation a notes application performs most often and optimizes for least, and that anything making long-form review less effortful earns its place.

Pausing has to be instant and obvious. The moment a reader wants to stop, any delay feels like fighting the software. The same is true in reverse: resuming should not require finding a control, only pressing the key that stopped it. The test, as always, is whether you forget the mechanism and remember the content.

### Failure modes to watch for

- A stalled frame followed by a sudden jump of many lines.
- The view snapping back after you scroll by hand.
- Text size changes that lose your place in the document.
- Reaching the end and silently freezing with no explanation.

> Motion is a poor substitute for interest, and an excellent companion to it.

## Section 25
A page that moves on its own changes where your attention goes, and mostly it goes to the text. There is an argument that this is a toy. There is a better argument that reading is the operation a notes application performs most often and optimizes for least, and that anything making long-form review less effortful earns its place.

Reversing direction is stranger than it sounds and more useful than expected. Something read three paragraphs ago becomes reachable without abandoning the mode, and the reader's place is preserved rather than lost to a scrollbar drag. None of this is novel; it is simply rare to find it applied to one's own notes.

## Section 26
Every archive eventually contains something its author no longer remembers writing. Line length turns out to matter as much as speed. A column that runs the full width of a modern display forces the eye to travel a long way back at every line break, and while the text is also moving vertically that return sweep becomes genuinely difficult. Narrower is better here than it is for static text.

Speed matters more than it seems. Too slow and attention wanders forward, hunting for the next line before it arrives. Too fast and comprehension quietly degrades while the sense of progress stays high, which is the worse failure of the two because it does not announce itself. The test, as always, is whether you forget the mechanism and remember the content.

## Section 27
There is a particular kind of note that only reveals its shape on a second pass, when nothing is being edited. The edges of the window are where the illusion breaks. A line that is abruptly cut in half by the top of the screen reads as an error rather than as motion. Softening those edges costs nothing and buys a surprising amount of comfort over a long document.

The edges of the window are where the illusion breaks. A line that is abruptly cut in half by the top of the screen reads as an error rather than as motion. Softening those edges costs nothing and buys a surprising amount of comfort over a long document. Whether that is worth a mode of its own is a question best answered by using it on something long.

#### A closer look (27)

Documents of this length are exactly where paging breaks down. Thirty screenfuls of scrolling is thirty small interruptions, each one an opportunity to lose the thread or decide the reading can wait.

## Section 28
There is a particular kind of note that only reveals its shape on a second pass, when nothing is being edited. Pausing has to be instant and obvious. The moment a reader wants to stop, any delay feels like fighting the software. The same is true in reverse: resuming should not require finding a control, only pressing the key that stopped it.

Pausing has to be instant and obvious. The moment a reader wants to stop, any delay feels like fighting the software. The same is true in reverse: resuming should not require finding a control, only pressing the key that stopped it. Whether that is worth a mode of its own is a question best answered by using it on something long.

### Things worth checking while it runs

- Does the text stay legible at the speed you chose, or does it only feel legible?
- Do headings and code blocks pass the reading line without jarring?
- Does pausing feel immediate?
- Does a manual scroll hand control back cleanly, without a fight on the next frame?

```rust
// Section 28: the motion is just arithmetic over a scroll offset.
let delta = speed * dt.clamp(0.0, 0.25);
offset += if direction_up { delta } else { -delta };
if offset >= max_scroll {
    if loop_at_end { offset = 0.0 } else { offset = max_scroll }
}
```

## Section 29
The first thing a reader does with an unfamiliar file is look for its edges. Reversing direction is stranger than it sounds and more useful than expected. Something read three paragraphs ago becomes reachable without abandoning the mode, and the reader's place is preserved rather than lost to a scrollbar drag.

Reversing direction is stranger than it sounds and more useful than expected. Something read three paragraphs ago becomes reachable without abandoning the mode, and the reader's place is preserved rather than lost to a scrollbar drag. The test, as always, is whether you forget the mechanism and remember the content.

## Section 30
The trouble with a long document is that it asks you to hold your place while your hands are busy elsewhere. Documents of this length are exactly where paging breaks down. Thirty screenfuls of scrolling is thirty small interruptions, each one an opportunity to lose the thread or decide the reading can wait.

There is an argument that this is a toy. There is a better argument that reading is the operation a notes application performs most often and optimizes for least, and that anything making long-form review less effortful earns its place. None of this is novel; it is simply rare to find it applied to one's own notes.

> The slowest part of reading is deciding to start.

## Section 31
The first thing a reader does with an unfamiliar file is look for its edges. Speed matters more than it seems. Too slow and attention wanders forward, hunting for the next line before it arrives. Too fast and comprehension quietly degrades while the sense of progress stays high, which is the worse failure of the two because it does not announce itself.

Documents of this length are exactly where paging breaks down. Thirty screenfuls of scrolling is thirty small interruptions, each one an opportunity to lose the thread or decide the reading can wait. None of this is novel; it is simply rare to find it applied to one's own notes.

## Section 32
The trouble with a long document is that it asks you to hold your place while your hands are busy elsewhere. The edges of the window are where the illusion breaks. A line that is abruptly cut in half by the top of the screen reads as an error rather than as motion. Softening those edges costs nothing and buys a surprising amount of comfort over a long document.

Speed matters more than it seems. Too slow and attention wanders forward, hunting for the next line before it arrives. Too fast and comprehension quietly degrades while the sense of progress stays high, which is the worse failure of the two because it does not announce itself. The test, as always, is whether you forget the mechanism and remember the content.

### Failure modes to watch for

- A stalled frame followed by a sudden jump of many lines.
- The view snapping back after you scroll by hand.
- Text size changes that lose your place in the document.
- Reaching the end and silently freezing with no explanation.

## Section 33
There is a particular kind of note that only reveals its shape on a second pass, when nothing is being edited. Speed matters more than it seems. Too slow and attention wanders forward, hunting for the next line before it arrives. Too fast and comprehension quietly degrades while the sense of progress stays high, which is the worse failure of the two because it does not announce itself.

Reversing direction is stranger than it sounds and more useful than expected. Something read three paragraphs ago becomes reachable without abandoning the mode, and the reader's place is preserved rather than lost to a scrollbar drag. Whether that is worth a mode of its own is a question best answered by using it on something long.

## Section 34
Reading at a fixed pace is a different act from scanning, and the difference shows up almost immediately. When the words arrive on a schedule you stop deciding when to move, and that small surrender is most of the effect. The eye settles into a band near the middle of the screen and stays there. Lines enter from below, cross the reading line, and leave. What would have been a series of small decisions about scrolling becomes one decision made once, at the start.

Line length turns out to matter as much as speed. A column that runs the full width of a modern display forces the eye to travel a long way back at every line break, and while the text is also moving vertically that return sweep becomes genuinely difficult. Narrower is better here than it is for static text. Whether that is worth a mode of its own is a question best answered by using it on something long.

## Section 35
Encryption is a boundary, not a filing system; the two get confused more often than they should. When the words arrive on a schedule you stop deciding when to move, and that small surrender is most of the effect. The eye settles into a band near the middle of the screen and stays there. Lines enter from below, cross the reading line, and leave. What would have been a series of small decisions about scrolling becomes one decision made once, at the start.

Speed matters more than it seems. Too slow and attention wanders forward, hunting for the next line before it arrives. Too fast and comprehension quietly degrades while the sense of progress stays high, which is the worse failure of the two because it does not announce itself. The test, as always, is whether you forget the mechanism and remember the content.

```rust
// Section 35: the motion is just arithmetic over a scroll offset.
let delta = speed * dt.clamp(0.0, 0.25);
offset += if direction_up { delta } else { -delta };
if offset >= max_scroll {
    if loop_at_end { offset = 0.0 } else { offset = max_scroll }
}
```

## Section 36
The first thing a reader does with an unfamiliar file is look for its edges. Line length turns out to matter as much as speed. A column that runs the full width of a modern display forces the eye to travel a long way back at every line break, and while the text is also moving vertically that return sweep becomes genuinely difficult. Narrower is better here than it is for static text.

Pausing has to be instant and obvious. The moment a reader wants to stop, any delay feels like fighting the software. The same is true in reverse: resuming should not require finding a control, only pressing the key that stopped it. It is a small feature with an unusually direct relationship to how the application actually gets used.

### Settings that change the feel most

- Speed — the obvious one, and the one worth adjusting per document.
- Column width — less obvious, and arguably more important.
- Text size — interacts with speed, since bigger text crosses the screen faster in lines per second.
- Edge fade — pure comfort, no function.

> The slowest part of reading is deciding to start.

#### A closer look (36)

Documents of this length are exactly where paging breaks down. Thirty screenfuls of scrolling is thirty small interruptions, each one an opportunity to lose the thread or decide the reading can wait.

## Section 37
Reading at a fixed pace is a different act from scanning, and the difference shows up almost immediately. Speed matters more than it seems. Too slow and attention wanders forward, hunting for the next line before it arrives. Too fast and comprehension quietly degrades while the sense of progress stays high, which is the worse failure of the two because it does not announce itself.

Documents of this length are exactly where paging breaks down. Thirty screenfuls of scrolling is thirty small interruptions, each one an opportunity to lose the thread or decide the reading can wait. None of this is novel; it is simply rare to find it applied to one's own notes.

## Section 38
Notes accumulate faster than they are revisited, which is the whole reason revisiting needs to be pleasant. Documents of this length are exactly where paging breaks down. Thirty screenfuls of scrolling is thirty small interruptions, each one an opportunity to lose the thread or decide the reading can wait.

Pausing has to be instant and obvious. The moment a reader wants to stop, any delay feels like fighting the software. The same is true in reverse: resuming should not require finding a control, only pressing the key that stopped it. Whether that is worth a mode of its own is a question best answered by using it on something long.

## Section 39
There is a particular kind of note that only reveals its shape on a second pass, when nothing is being edited. Speed matters more than it seems. Too slow and attention wanders forward, hunting for the next line before it arrives. Too fast and comprehension quietly degrades while the sense of progress stays high, which is the worse failure of the two because it does not announce itself.

Reversing direction is stranger than it sounds and more useful than expected. Something read three paragraphs ago becomes reachable without abandoning the mode, and the reader's place is preserved rather than lost to a scrollbar drag. It is a small feature with an unusually direct relationship to how the application actually gets used.

## Section 40
Notes accumulate faster than they are revisited, which is the whole reason revisiting needs to be pleasant. Line length turns out to matter as much as speed. A column that runs the full width of a modern display forces the eye to travel a long way back at every line break, and while the text is also moving vertically that return sweep becomes genuinely difficult. Narrower is better here than it is for static text.

When the words arrive on a schedule you stop deciding when to move, and that small surrender is most of the effect. The eye settles into a band near the middle of the screen and stays there. Lines enter from below, cross the reading line, and leave. What would have been a series of small decisions about scrolling becomes one decision made once, at the start. The test, as always, is whether you forget the mechanism and remember the content.

### Settings that change the feel most

- Speed — the obvious one, and the one worth adjusting per document.
- Column width — less obvious, and arguably more important.
- Text size — interacts with speed, since bigger text crosses the screen faster in lines per second.
- Edge fade — pure comfort, no function.

## Section 41
Encryption is a boundary, not a filing system; the two get confused more often than they should. Line length turns out to matter as much as speed. A column that runs the full width of a modern display forces the eye to travel a long way back at every line break, and while the text is also moving vertically that return sweep becomes genuinely difficult. Narrower is better here than it is for static text.

When the words arrive on a schedule you stop deciding when to move, and that small surrender is most of the effect. The eye settles into a band near the middle of the screen and stays there. Lines enter from below, cross the reading line, and leave. What would have been a series of small decisions about scrolling becomes one decision made once, at the start. It is a small feature with an unusually direct relationship to how the application actually gets used.

## Section 42
Reading at a fixed pace is a different act from scanning, and the difference shows up almost immediately. Pausing has to be instant and obvious. The moment a reader wants to stop, any delay feels like fighting the software. The same is true in reverse: resuming should not require finding a control, only pressing the key that stopped it.

Reversing direction is stranger than it sounds and more useful than expected. Something read three paragraphs ago becomes reachable without abandoning the mode, and the reader's place is preserved rather than lost to a scrollbar drag. The test, as always, is whether you forget the mechanism and remember the content.

> The slowest part of reading is deciding to start.

```rust
// Section 42: the motion is just arithmetic over a scroll offset.
let delta = speed * dt.clamp(0.0, 0.25);
offset += if direction_up { delta } else { -delta };
if offset >= max_scroll {
    if loop_at_end { offset = 0.0 } else { offset = max_scroll }
}
```

## Section 43
Every archive eventually contains something its author no longer remembers writing. Reversing direction is stranger than it sounds and more useful than expected. Something read three paragraphs ago becomes reachable without abandoning the mode, and the reader's place is preserved rather than lost to a scrollbar drag.

The edges of the window are where the illusion breaks. A line that is abruptly cut in half by the top of the screen reads as an error rather than as motion. Softening those edges costs nothing and buys a surprising amount of comfort over a long document. The test, as always, is whether you forget the mechanism and remember the content.

## Section 44
Every archive eventually contains something its author no longer remembers writing. There is an argument that this is a toy. There is a better argument that reading is the operation a notes application performs most often and optimizes for least, and that anything making long-form review less effortful earns its place.

The edges of the window are where the illusion breaks. A line that is abruptly cut in half by the top of the screen reads as an error rather than as motion. Softening those edges costs nothing and buys a surprising amount of comfort over a long document. The test, as always, is whether you forget the mechanism and remember the content.

### Settings that change the feel most

- Speed — the obvious one, and the one worth adjusting per document.
- Column width — less obvious, and arguably more important.
- Text size — interacts with speed, since bigger text crosses the screen faster in lines per second.
- Edge fade — pure comfort, no function.

## Section 45
Notes accumulate faster than they are revisited, which is the whole reason revisiting needs to be pleasant. Reversing direction is stranger than it sounds and more useful than expected. Something read three paragraphs ago becomes reachable without abandoning the mode, and the reader's place is preserved rather than lost to a scrollbar drag.

When the words arrive on a schedule you stop deciding when to move, and that small surrender is most of the effect. The eye settles into a band near the middle of the screen and stays there. Lines enter from below, cross the reading line, and leave. What would have been a series of small decisions about scrolling becomes one decision made once, at the start. Whether that is worth a mode of its own is a question best answered by using it on something long.

#### A closer look (45)

Pausing has to be instant and obvious. The moment a reader wants to stop, any delay feels like fighting the software. The same is true in reverse: resuming should not require finding a control, only pressing the key that stopped it.

## Section 46
Notes accumulate faster than they are revisited, which is the whole reason revisiting needs to be pleasant. Pausing has to be instant and obvious. The moment a reader wants to stop, any delay feels like fighting the software. The same is true in reverse: resuming should not require finding a control, only pressing the key that stopped it.

The edges of the window are where the illusion breaks. A line that is abruptly cut in half by the top of the screen reads as an error rather than as motion. Softening those edges costs nothing and buys a surprising amount of comfort over a long document. It is a small feature with an unusually direct relationship to how the application actually gets used.

## Section 47
Notes accumulate faster than they are revisited, which is the whole reason revisiting needs to be pleasant. Reversing direction is stranger than it sounds and more useful than expected. Something read three paragraphs ago becomes reachable without abandoning the mode, and the reader's place is preserved rather than lost to a scrollbar drag.

Reversing direction is stranger than it sounds and more useful than expected. Something read three paragraphs ago becomes reachable without abandoning the mode, and the reader's place is preserved rather than lost to a scrollbar drag. Whether that is worth a mode of its own is a question best answered by using it on something long.

## Section 48
Every archive eventually contains something its author no longer remembers writing. Speed matters more than it seems. Too slow and attention wanders forward, hunting for the next line before it arrives. Too fast and comprehension quietly degrades while the sense of progress stays high, which is the worse failure of the two because it does not announce itself.

The edges of the window are where the illusion breaks. A line that is abruptly cut in half by the top of the screen reads as an error rather than as motion. Softening those edges costs nothing and buys a surprising amount of comfort over a long document. None of this is novel; it is simply rare to find it applied to one's own notes.

### Things worth checking while it runs

- Does the text stay legible at the speed you chose, or does it only feel legible?
- Do headings and code blocks pass the reading line without jarring?
- Does pausing feel immediate?
- Does a manual scroll hand control back cleanly, without a fight on the next frame?

> The slowest part of reading is deciding to start.

## Section 49
Every archive eventually contains something its author no longer remembers writing. Documents of this length are exactly where paging breaks down. Thirty screenfuls of scrolling is thirty small interruptions, each one an opportunity to lose the thread or decide the reading can wait.

When the words arrive on a schedule you stop deciding when to move, and that small surrender is most of the effect. The eye settles into a band near the middle of the screen and stays there. Lines enter from below, cross the reading line, and leave. What would have been a series of small decisions about scrolling becomes one decision made once, at the start. None of this is novel; it is simply rare to find it applied to one's own notes.

```rust
// Section 49: the motion is just arithmetic over a scroll offset.
let delta = speed * dt.clamp(0.0, 0.25);
offset += if direction_up { delta } else { -delta };
if offset >= max_scroll {
    if loop_at_end { offset = 0.0 } else { offset = max_scroll }
}
```

## Section 50
Encryption is a boundary, not a filing system; the two get confused more often than they should. Speed matters more than it seems. Too slow and attention wanders forward, hunting for the next line before it arrives. Too fast and comprehension quietly degrades while the sense of progress stays high, which is the worse failure of the two because it does not announce itself.

Speed matters more than it seems. Too slow and attention wanders forward, hunting for the next line before it arrives. Too fast and comprehension quietly degrades while the sense of progress stays high, which is the worse failure of the two because it does not announce itself. None of this is novel; it is simply rare to find it applied to one's own notes.

## Section 51
Every archive eventually contains something its author no longer remembers writing. Documents of this length are exactly where paging breaks down. Thirty screenfuls of scrolling is thirty small interruptions, each one an opportunity to lose the thread or decide the reading can wait.

Line length turns out to matter as much as speed. A column that runs the full width of a modern display forces the eye to travel a long way back at every line break, and while the text is also moving vertically that return sweep becomes genuinely difficult. Narrower is better here than it is for static text. None of this is novel; it is simply rare to find it applied to one's own notes.

## Section 52
Encryption is a boundary, not a filing system; the two get confused more often than they should. Speed matters more than it seems. Too slow and attention wanders forward, hunting for the next line before it arrives. Too fast and comprehension quietly degrades while the sense of progress stays high, which is the worse failure of the two because it does not announce itself.

There is an argument that this is a toy. There is a better argument that reading is the operation a notes application performs most often and optimizes for least, and that anything making long-form review less effortful earns its place. None of this is novel; it is simply rare to find it applied to one's own notes.

### Failure modes to watch for

- A stalled frame followed by a sudden jump of many lines.
- The view snapping back after you scroll by hand.
- Text size changes that lose your place in the document.
- Reaching the end and silently freezing with no explanation.

## Section 53
Reading at a fixed pace is a different act from scanning, and the difference shows up almost immediately. Line length turns out to matter as much as speed. A column that runs the full width of a modern display forces the eye to travel a long way back at every line break, and while the text is also moving vertically that return sweep becomes genuinely difficult. Narrower is better here than it is for static text.

Line length turns out to matter as much as speed. A column that runs the full width of a modern display forces the eye to travel a long way back at every line break, and while the text is also moving vertically that return sweep becomes genuinely difficult. Narrower is better here than it is for static text. The test, as always, is whether you forget the mechanism and remember the content.

## Section 54
The trouble with a long document is that it asks you to hold your place while your hands are busy elsewhere. Line length turns out to matter as much as speed. A column that runs the full width of a modern display forces the eye to travel a long way back at every line break, and while the text is also moving vertically that return sweep becomes genuinely difficult. Narrower is better here than it is for static text.

Documents of this length are exactly where paging breaks down. Thirty screenfuls of scrolling is thirty small interruptions, each one an opportunity to lose the thread or decide the reading can wait. The test, as always, is whether you forget the mechanism and remember the content.

> Motion is a poor substitute for interest, and an excellent companion to it.

#### A closer look (54)

Documents of this length are exactly where paging breaks down. Thirty screenfuls of scrolling is thirty small interruptions, each one an opportunity to lose the thread or decide the reading can wait.

## Section 55
Encryption is a boundary, not a filing system; the two get confused more often than they should. Line length turns out to matter as much as speed. A column that runs the full width of a modern display forces the eye to travel a long way back at every line break, and while the text is also moving vertically that return sweep becomes genuinely difficult. Narrower is better here than it is for static text.

Line length turns out to matter as much as speed. A column that runs the full width of a modern display forces the eye to travel a long way back at every line break, and while the text is also moving vertically that return sweep becomes genuinely difficult. Narrower is better here than it is for static text. Whether that is worth a mode of its own is a question best answered by using it on something long.

## Section 56
The trouble with a long document is that it asks you to hold your place while your hands are busy elsewhere. Speed matters more than it seems. Too slow and attention wanders forward, hunting for the next line before it arrives. Too fast and comprehension quietly degrades while the sense of progress stays high, which is the worse failure of the two because it does not announce itself.

Line length turns out to matter as much as speed. A column that runs the full width of a modern display forces the eye to travel a long way back at every line break, and while the text is also moving vertically that return sweep becomes genuinely difficult. Narrower is better here than it is for static text. None of this is novel; it is simply rare to find it applied to one's own notes.

### Things worth checking while it runs

- Does the text stay legible at the speed you chose, or does it only feel legible?
- Do headings and code blocks pass the reading line without jarring?
- Does pausing feel immediate?
- Does a manual scroll hand control back cleanly, without a fight on the next frame?

```rust
// Section 56: the motion is just arithmetic over a scroll offset.
let delta = speed * dt.clamp(0.0, 0.25);
offset += if direction_up { delta } else { -delta };
if offset >= max_scroll {
    if loop_at_end { offset = 0.0 } else { offset = max_scroll }
}
```

## Section 57
Every archive eventually contains something its author no longer remembers writing. When the words arrive on a schedule you stop deciding when to move, and that small surrender is most of the effect. The eye settles into a band near the middle of the screen and stays there. Lines enter from below, cross the reading line, and leave. What would have been a series of small decisions about scrolling becomes one decision made once, at the start.

Pausing has to be instant and obvious. The moment a reader wants to stop, any delay feels like fighting the software. The same is true in reverse: resuming should not require finding a control, only pressing the key that stopped it. The test, as always, is whether you forget the mechanism and remember the content.

## Section 58
A page that moves on its own changes where your attention goes, and mostly it goes to the text. The edges of the window are where the illusion breaks. A line that is abruptly cut in half by the top of the screen reads as an error rather than as motion. Softening those edges costs nothing and buys a surprising amount of comfort over a long document.

Reversing direction is stranger than it sounds and more useful than expected. Something read three paragraphs ago becomes reachable without abandoning the mode, and the reader's place is preserved rather than lost to a scrollbar drag. It is a small feature with an unusually direct relationship to how the application actually gets used.

## Section 59
The first thing a reader does with an unfamiliar file is look for its edges. Line length turns out to matter as much as speed. A column that runs the full width of a modern display forces the eye to travel a long way back at every line break, and while the text is also moving vertically that return sweep becomes genuinely difficult. Narrower is better here than it is for static text.

When the words arrive on a schedule you stop deciding when to move, and that small surrender is most of the effect. The eye settles into a band near the middle of the screen and stays there. Lines enter from below, cross the reading line, and leave. What would have been a series of small decisions about scrolling becomes one decision made once, at the start. It is a small feature with an unusually direct relationship to how the application actually gets used.

## Section 60
Notes accumulate faster than they are revisited, which is the whole reason revisiting needs to be pleasant. There is an argument that this is a toy. There is a better argument that reading is the operation a notes application performs most often and optimizes for least, and that anything making long-form review less effortful earns its place.

Line length turns out to matter as much as speed. A column that runs the full width of a modern display forces the eye to travel a long way back at every line break, and while the text is also moving vertically that return sweep becomes genuinely difficult. Narrower is better here than it is for static text. The test, as always, is whether you forget the mechanism and remember the content.

### Settings that change the feel most

- Speed — the obvious one, and the one worth adjusting per document.
- Column width — less obvious, and arguably more important.
- Text size — interacts with speed, since bigger text crosses the screen faster in lines per second.
- Edge fade — pure comfort, no function.

> Motion is a poor substitute for interest, and an excellent companion to it.

---

## The End

If you reached this line by reading rather than by pressing `End`, the
mode works. If you reached it by pressing `End`, that works too.

By default the crawl turns around here and heads back. Settings can make
it start over from the top instead, or stop — and even stopped, `Space`
or `R` sets it moving again.
