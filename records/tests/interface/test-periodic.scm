(lambda (make-interface-harness)
  (with-let (make-interface-harness
             :journals 1
             :users '(admin alice)
             :admins '((*state* admin)))

    (define periodic-path '(*state* *periodic*))
    (define periodic-program
      '(lambda (journal index)
         (let* ((path (list '*state* 'periodic-runs index))
                (current ((journal 'get) path))
                (next (if (equal? current '(nothing)) 1 (+ current 1))))
           ((journal 'set!) path next)
           next)))
    (define (error-result? result)
      (and (pair? result) (eq? (car result) 'error)))

    (test-report)

    ;; Authorization policy cannot grant writes to the reserved periodic path;
    ;; only Root or a configured local Interface administrator may edit it.
    (test-submit
      ((*journal* journal-1 'authorize!)
       '((user (*state* *periodic*))
         (rule ((principal (*state* alice)) (path ())
                (get #t) (set! #t) (resolve #t)))))
      :expect #t)
    (test-submit ((alice journal-1 'set!) periodic-path periodic-program)
      :expect error-result?)
    (test-submit
      ((alice journal-1 'set-batch!)
       `((paths (,periodic-path)) (values (,periodic-program)) (expression? #t)))
      :expect error-result?)
    (test-submit ((admin journal-1 'set!) periodic-path periodic-program)
      :expect #t)
    (test-report)

    ;; The internal mutating continuation commits the program but never launches
    ;; it separately. Only the non-mutating outer step dispatches one detached
    ;; call, so the index-specific count remains exactly one.
    (test-submit ((*journal* journal-1 'step!)) :expect 1)
    (test-report)
    (test-submit ((*journal* journal-1 'get) '(*state* periodic-runs 0))
      :expect 1)

    ;; Rotation updates both the stored credential and verifier. The next outer
    ;; step reenters call! with the new value and launches index 1 exactly once.
    (test-submit
      ((*journal* journal-1 '*secret*) '((secret "periodic-interface-v2")))
      :expect #t)
    (test-report)
    (journal-1 'credentials "periodic-interface-v2")
    (test-submit ((*journal* journal-1 'step!)) :expect 2)
    (test-report)
    (test-submit ((*journal* journal-1 'get) '(*state* periodic-runs 0))
      :expect 1)
    (test-submit ((*journal* journal-1 'get) '(*state* periodic-runs 1))
      :expect 1)

    ;; Concurrent outer steps may overlap and make their internal commits retry,
    ;; but each successful committed index still receives one launch.
    (test-submit ((*journal* journal-1 'step!))
      :schedule '(2 1) :expect integer?)
    (test-submit ((*journal* journal-1 'step!))
      :schedule '(1 2) :expect integer?)
    (test-report)
    (test-submit ((*journal* journal-1 'get) '(*state* periodic-runs 2))
      :expect 1)
    (test-submit ((*journal* journal-1 'get) '(*state* periodic-runs 3))
      :expect 1)

    ;; Deleting the staged program disables future launches. The deletion is
    ;; committed by the same outer step that observes the missing staged value.
    (test-submit ((admin journal-1 'set!) periodic-path '(nothing))
      :expect #t)
    (test-submit ((*journal* journal-1 'step!)) :expect 5)
    (test-report)
    (test-submit ((*journal* journal-1 'get) '(*state* periodic-runs 4))
      :expect '(nothing))

    ;; A read-only program creates one new index when installed, then repeated
    ;; unchanged steps report no new commit and must not relaunch that index.
    (test-submit
      ((admin journal-1 'set!) periodic-path '(lambda (journal index) index))
      :expect #t)
    (test-submit ((*journal* journal-1 'step!)) :expect 6)
    (test-report)
    (test-submit
      (raw journal-1 '(*step* "pass-1" (ledger-step #t #t)))
      :expect '(6 #f #t))
    (test-submit ((*journal* journal-1 'step!)) :expect 6)

    (test-report)))
