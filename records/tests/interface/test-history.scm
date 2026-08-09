(lambda (make-interface-harness)

  (with-let (make-interface-harness :journals 2 :users '(alice bob))

    (define* (collect action (schedule '()))
      (test-report)
      (test-submit action :schedule schedule)
      (test-await))

    (define alice-principal '(journal-1 *state* alice))
    (define bob-principal '(journal-1 *state* bob))

    (test-report)

    ;; Commit the first terminal value and its authorization before establishing
    ;; the reciprocal relationship.
    (test-submit ((*journal* journal-1 'set!) '(*state* alice seed) "origin") :expect #t)
    (test-submit ((*journal* journal-2 'set!) '(*state* bob document) "version-1") :expect #t)
    (test-submit
      ((*journal* journal-2 'authorize!)
       `((user (*state* bob))
         (rule ((principal ,alice-principal) (key-index (-10 -1))
                (path (document)) (get #t) (set! #t) (resolve #t)))))
      :expect #t)
    (test-submit
      ((*journal* journal-2 'authorize!)
       `((user (*state* bob))
         (rule ((principal ,bob-principal) (key-index (-10 -1))
                (path (document)) (get #t) (set! #t) (resolve #t)))))
      :expect #t)
    (test-submit ((*journal* journal-1 'step!)) :expect 1)
    (test-submit ((*journal* journal-2 'step!)) :expect 1)
    (test-report)

    ;; The acceptor must commit its reverse view before the initiator can retain
    ;; a terminal object that proves the complete route back to Alice.
    (test-submit ((*journal* journal-1 'bridge!) journal-2) :schedule '(2 1) :expect #t)
    (test-report)
    (test-submit ((*journal* journal-2 'step!)) :expect 2)
    (test-submit ((*journal* journal-1 'bridge!) journal-2) :schedule '(1 2) :tick 1 :expect #t)
    (test-report)
    (test-submit ((*journal* journal-1 'step!)) :expect 2)
    (test-report)

    (test-submit ((alice journal-1 journal-2 'get) '(*state* bob document)) :schedule '(2 1 0 1) :expect "version-1")
    (test-submit
      ((alice journal-1 journal-2 'resolve)
       '(-1 *state* bob document) :pinned? #f :proof? #f)
      :schedule '(1 2 1 0) :expect "version-1")
    (test-report)

    ;; The terminal can advance independently. Stage reads see its live value,
    ;; while the origin's committed route continues resolving version 1.
    (test-submit ((*journal* journal-2 'set!) '(*state* bob document) "version-2") :expect #t)
    (test-submit ((*journal* journal-2 'step!)) :expect 3)
    (test-submit ((alice journal-1 journal-2 'get) '(*state* bob document)) :schedule '(2 1 0 1) :expect "version-2")
    (test-submit
      ((alice journal-1 journal-2 'resolve)
       '(-1 *state* bob document) :pinned? #f :proof? #f)
      :schedule '(1 2 1 0) :expect "version-1")
    (test-report)

    ;; Synchronization makes version 2 part of a new origin head without
    ;; invalidating exact selection of terminal index 1.
    (test-submit ((*journal* journal-1 'bridge!) journal-2) :schedule '(3 1) :expect #t)
    (test-report)
    (test-submit ((*journal* journal-1 'step!)) :expect 3)
    (test-report)

    (test-submit
      ((alice journal-1 journal-2 'resolve)
       '(-1 *state* bob document) :pinned? #f :proof? #f)
      :schedule '(2 1 0 1) :expect "version-2")
    (test-submit
      ((alice journal-1 journal-2 'resolve :history '(2 1))
       '(-1 *state* bob document) :pinned? #f :proof? #f)
      :schedule '(1 3 1 0) :expect "version-1")

    ;; An exact historical resolve remains stable while an unrelated origin
    ;; commit and its scheduled synchronization complete first.
    (test-submit
      ((alice journal-1 journal-2 'resolve :history '(2 1))
       '(-1 *state* bob document) :pinned? #f :proof? #f)
      :schedule '(4 2 1 0) :expect "version-1")
    (test-submit ((*journal* journal-1 'set!) '(*state* alice marker) "after-resolve") :tick 1 :expect #t)
    (test-submit ((*journal* journal-1 'step!)) :schedule '(1 2) :tick 1 :expect 4)
    (test-report)

    ;; The proof is selected from origin index 3 and terminal index 1. Advance
    ;; the origin once more before pinning to exercise retained-history anchoring.
    (let ((path '(3 journal-2 1 *state* bob document)))
      (test-submit ((*journal* journal-1 'set!) '(*state* alice marker-2) "before-pin") :expect #t)
      (test-submit ((*journal* journal-1 'step!)) :schedule '(1 2) :tick 1 :expect 5)
      (test-submit
        ((bob journal-1 journal-2 'pin! :history '(3 1))
         '(-1 *state* bob document))
        :schedule '(2 1 1 0) :tick 1 :expect #t)
      (test-report)
      (test-submit
        ((bob journal-1 journal-2 'resolve :history '(3 1))
         '(-1 *state* bob document) :pinned? #t :proof? #f)
        :expect '((content "version-1") (pinned? #t)))
      (test-submit ((bob journal-1 'unpin!) path) :expect #t)
      (test-submit
        ((bob journal-1 journal-2 'resolve :history '(3 1))
         '(-1 *state* bob document) :pinned? #t :proof? #f)
        :expect '((content "version-1") (pinned? #f)))
      (test-submit
        ((*journal* journal-2 'resolve)
         '(1 *state* bob document) :pinned? #t :proof? #f)
        :expect (lambda (result)
                  (and (equal? (cadr (assoc 'content result)) "version-1")
                       (not (cadr (assoc 'pinned? result))))))
      (test-report))

    ;; A third terminal version and a delayed old-view resolve overlap normal
    ;; synchronization. Until the origin steps, resolve still selects version 2.
    (test-submit ((*journal* journal-2 'set!) '(*state* bob document) "version-3") :expect #t)
    (test-submit ((*journal* journal-2 'step!)) :expect 4)
    (test-report)
    (test-submit
      ((alice journal-1 journal-2 'resolve)
       '(-1 *state* bob document) :pinned? #f :proof? #f)
      :schedule '(4 2 1 0) :expect "version-2")
    (test-submit ((*journal* journal-1 'bridge!) journal-2) :schedule '(1 3) :tick 1 :expect #t)
    (test-report)
    (test-submit ((*journal* journal-1 'step!)) :expect 6)
    (test-submit
      ((alice journal-1 journal-2 'resolve)
       '(-1 *state* bob document) :pinned? #f :proof? #f)
      :schedule '(2 1 0 1) :tick 1 :expect "version-3")

    (test-report))

  ;; A separate three-journal world exercises explicit history selection and
  ;; origin-local retention at every hop of a multi-hop route.
  (with-let (make-interface-harness :journals 3 :journal-start 4 :users '(alice bob))

    (define* (collect action (schedule '()))
      (test-report)
      (test-submit action :schedule schedule)
      (test-await))

    (define alice-principal
      '(journal-5 journal-4 *state* alice))
    (define bob-principal
      '(journal-5 journal-4 *state* bob))

    (test-report)
    (test-submit ((*journal* journal-4 'set!) '(*state* alice seed) "origin") :expect #t)
    (test-submit ((*journal* journal-5 'set!) '(*state* alice seed) "middle") :expect #t)
    (test-submit ((*journal* journal-6 'set!) '(*state* bob document) "multi-v1") :expect #t)
    (test-submit
      ((*journal* journal-6 'authorize!)
       `((user (*state* bob))
         (rule ((principal ,alice-principal) (key-index (-20 -1))
                (path (document)) (get #t) (set! #f) (resolve #t)))))
      :expect #t)
    (test-submit
      ((*journal* journal-6 'authorize!)
       `((user (*state* bob))
         (rule ((principal ,bob-principal) (key-index (-20 -1))
                (path (document)) (get #t) (set! #f) (resolve #t)))))
      :expect #t)
    (for-each
      (lambda (journal) (test-submit ((*journal* journal 'step!)) :expect 1))
      (list journal-4 journal-5 journal-6))
    (test-report)

    ;; Commit the reverse route first, then carry the terminal view back through
    ;; journal-5 into journal-4.
    (test-submit ((*journal* journal-4 'bridge!) journal-5) :schedule '(2 1) :expect #t)
    (test-report)
    (test-submit ((*journal* journal-5 'step!)) :expect 2)
    (test-submit ((*journal* journal-5 'bridge!) journal-6) :schedule '(1 2) :tick 1 :expect #t)
    (test-report)
    (test-submit ((*journal* journal-6 'step!)) :expect 2)
    (test-submit ((*journal* journal-5 'step!)) :expect 3)
    (test-report)
    (test-submit ((*journal* journal-5 'bridge!) journal-6) :schedule '(2 1) :tick 1 :expect #t)
    (test-report)
    (test-submit ((*journal* journal-5 'step!)) :expect 3)
    (test-submit ((*journal* journal-4 'bridge!) journal-5) :schedule '(1 2) :tick 1 :expect #t)
    (test-report)
    (test-submit ((*journal* journal-4 'step!)) :expect 2)
    (test-report)

    (test-submit
      ((alice journal-4 journal-5 journal-6 'resolve)
       '(-1 *state* bob document) :pinned? #f :proof? #f)
      :schedule '(2 1 2 0 1 2) :expect "multi-v1")

    ;; The terminal advances to version 2, then the newer view propagates through
    ;; both bridge histories. Explicit older hop indexes still select version 1.
    (test-submit ((*journal* journal-6 'set!) '(*state* bob document) "multi-v2") :expect #t)
    (test-submit ((*journal* journal-6 'step!)) :expect 3)
    (test-report)
    (test-submit ((*journal* journal-5 'bridge!) journal-6) :schedule '(2 1) :expect #t)
    (test-report)
    (test-submit ((*journal* journal-5 'step!)) :expect 4)
    (test-submit ((*journal* journal-4 'bridge!) journal-5) :schedule '(1 2) :tick 1 :expect #t)
    (test-report)
    (test-submit ((*journal* journal-4 'step!)) :expect 3)
    (test-report)

    (test-submit
      ((alice journal-4 journal-5 journal-6 'resolve)
       '(-1 *state* bob document) :pinned? #f :proof? #f)
      :schedule '(2 1 0 2 1 0) :expect "multi-v2")
    (test-submit
      ((alice journal-4 journal-5 journal-6 'resolve :history '(2 2 0))
       '(-1 *state* bob document) :pinned? #f :proof? #f)
      :schedule '(1 3 1 0 2 1) :expect "multi-v1")
    (test-report)

    (let ((path '(2 journal-5 2 journal-6 0
                  *state* bob document)))
      (test-submit ((*journal* journal-4 'set!) '(*state* alice marker) "after-proof") :expect #t)
      (test-submit ((*journal* journal-4 'step!)) :schedule '(1 2) :tick 1 :expect 4)
      (test-submit
        ((bob journal-4 journal-5 journal-6 'pin! :history '(2 2 0))
         '(-1 *state* bob document))
        :schedule '(2 1 2 0 1 0) :tick 1 :expect #t)
      (test-report)
      (test-submit
        ((bob journal-4 journal-5 journal-6 'resolve :history '(2 2 0))
         '(-1 *state* bob document) :pinned? #t :proof? #f)
        :expect '((content "multi-v1") (pinned? #t)))
      (test-submit ((bob journal-4 'unpin!) path) :expect #t)
      (test-submit
        ((bob journal-4 journal-5 journal-6 'resolve :history '(2 2 0))
         '(-1 *state* bob document) :pinned? #t :proof? #f)
        :expect '((content "multi-v1") (pinned? #f))))

    (test-report))

  ;; Shrinking from unlimited retention must destructively prune discarded
  ;; temporary history while preserving permanent path-level pins. Later
  ;; widening cannot resurrect an unpinned sibling or hide the pinned proof.
  (with-let (make-interface-harness :journals 1 :journal-start 8
                                    :users '(alice) :window #f)

    (test-report)

    (let loop ((version 0))
      (if (< version 3)
          (begin
            (test-submit ((*journal* journal-8 'set!) '(*state* retained unpinned) version) :expect #t)
            (test-submit
              ((*journal* journal-8 'set!)
               '(*state* retained pinned)
               (append "p" (number->string version)))
              :expect #t)
            (test-submit ((*journal* journal-8 'step!)) :expect (+ version 1))
            (test-report)
            (loop (+ version 1)))))

    (test-submit ((*journal* journal-8 'pin!) '(0 *state* retained pinned)) :expect #t)
    (test-submit
      ((*journal* journal-8 'resolve)
       '(0 *state* retained pinned) :pinned? #t :proof? #f)
      :expect '((content "p0") (pinned? #t)))
    (test-submit ((*journal* journal-8 '*window-set*) '((value 2))) :expect #t)
    (test-report)
    (test-submit
      ((*journal* journal-8 'resolve)
       '(0 *state* retained unpinned) :pinned? #f :proof? #f)
      :expect '(unknown))
    (test-submit
      ((*journal* journal-8 'resolve)
       '(0 *state* retained pinned) :pinned? #t :proof? #f)
      :expect '((content "p0") (pinned? #t)))
    (test-submit ((*journal* journal-8 'resolve) '(2 *state* retained unpinned) :pinned? #f :proof? #f) :expect 2)
    (test-report)

    (test-submit ((*journal* journal-8 '*window-set*) '((value 10))) :expect #t)
    (test-submit
      ((*journal* journal-8 'resolve)
       '(0 *state* retained unpinned) :pinned? #f :proof? #f)
      :expect '(unknown))
    (test-submit ((*journal* journal-8 'resolve) '(2 *state* retained unpinned) :pinned? #f :proof? #f) :expect 2)
    (test-submit
      ((*journal* journal-8 'resolve)
       '(0 *state* retained pinned) :pinned? #t :proof? #f)
      :expect '((content "p0") (pinned? #t)))
    (test-report)

    ;; Removing the finite bound is another widening transition. It likewise
    ;; preserves pins without restoring discarded siblings.
    (test-submit
      ((*journal* journal-8 'update-config!)
       '((path (public window)) (value #f))) :expect #t)
    (test-submit
      ((*journal* journal-8 'resolve)
       '(0 *state* retained unpinned) :pinned? #f :proof? #f)
      :expect '(unknown))
    (test-submit ((*journal* journal-8 'resolve) '(2 *state* retained unpinned) :pinned? #f :proof? #f) :expect 2)
    (test-submit
      ((*journal* journal-8 'resolve)
       '(0 *state* retained pinned) :pinned? #t :proof? #f)
      :expect '((content "p0") (pinned? #t)))
    (test-submit ((*journal* journal-8 'unpin!) '(0 *state* retained pinned)) :expect #t)
    (test-submit
      ((*journal* journal-8 'resolve)
       '(0 *state* retained pinned) :pinned? #t :proof? #f)
      :expect '(unknown))

    (test-report)))
