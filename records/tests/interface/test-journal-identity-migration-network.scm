(lambda (make-interface-harness)
  (with-let (make-interface-harness :journals 3 :users '(alice) :legacy? #t)

    (define (error-result? result)
      (and (pair? result) (eq? (car result) 'error)))

    (test-report)

    (for-each
     (lambda (journal)
       (test-submit
        ((*journal* journal 'set!) '(*state* network seed) (journal 'name))
        :expect #t)
       (test-submit ((*journal* journal 'step!)) :expect 1))
     (list journal-1 journal-2 journal-3))
    (test-report)

    (test-submit ((*journal* journal-1 'bridge!) journal-2) :expect #t)
    (test-submit ((*journal* journal-2 'bridge!) journal-3) :expect #t)
    (test-report)

    (for-each
     (lambda (journal)
       (test-submit ((*journal* journal 'step!)) :expect integer?))
     (list journal-1 journal-2 journal-3))
    (test-report)

    ;; Propagate the terminal relationship and then the complete reverse key path
    ;; back through the middle Journal.
    (test-submit ((*journal* journal-2 'bridge!) journal-3) :expect #t)
    (test-report)
    (test-submit ((*journal* journal-2 'step!)) :expect integer?)
    (test-submit ((*journal* journal-1 'bridge!) journal-2) :expect #t)
    (test-report)
    (test-submit ((*journal* journal-1 'step!)) :expect integer?)
    (test-report)
    (test-submit ((*journal* journal-3 'step!)) :expect integer?)
    (test-submit ((*journal* journal-2 'bridge!) journal-3) :expect #t)
    (test-report)
    (test-submit ((*journal* journal-2 'step!)) :expect integer?)
    (test-submit ((*journal* journal-1 'bridge!) journal-2) :expect #t)
    (test-report)
    (test-submit ((*journal* journal-1 'step!)) :expect integer?)
    (test-report)

    (for-each
     (lambda (journal)
       (test-submit (update-interface journal '() 4 #f)
                    :expect "Installed interface"))
     (list journal-1 journal-2 journal-3))
    (for-each
     (lambda (journal)
       (test-submit
        ((*journal* journal 'authorize!)
         '((user (*state* network))
           (rule ((principal (journal-2 journal-1 *state* alice))
                  (key-index (-20 -1)) (path (seed))
                  (put! #f) (use! ((read-only? #t))) (run! #f)
                  (retrieve #t)))))
        :expect #t))
     (list journal-1 journal-3))
    (test-report)

    ;; Reopen the migrated graph through ordinary old-head exchange and
    ;; convergence; no identity shortcut or rebuilt relationship is involved.
    (test-submit ((*journal* journal-2 'bridge!) journal-3) :expect #t)
    (test-report)
    (test-submit ((*journal* journal-3 'step!)) :expect integer?)
    (test-submit ((*journal* journal-2 'step!)) :expect integer?)
    (test-report)
    (test-submit ((*journal* journal-1 'bridge!) journal-2) :expect #t)
    (test-report)
    (test-submit ((*journal* journal-1 'step!)) :expect integer?)
    (test-submit ((*journal* journal-2 'step!)) :expect integer?)
    (test-report)
    (test-submit ((*journal* journal-2 'bridge!) journal-3) :expect #t)
    (test-report)
    (test-submit ((*journal* journal-2 'step!)) :expect integer?)
    (test-submit ((*journal* journal-1 'bridge!) journal-2) :expect #t)
    (test-report)
    (test-submit ((*journal* journal-1 'step!)) :expect integer?)
    (test-report)
    (test-submit
     ((alice journal-1 journal-2 journal-3 'use!) '(*state* network seed))
     :expect 'journal-3)
    (test-submit
     ((alice journal-1 journal-2 journal-1 'use!) '(*state* network seed))
     :expect 'journal-1)
    (test-report)

    ;; Extend every lineage with an identity-free v2 head and reconverge.
    (for-each
     (lambda (journal)
       (test-submit
        ((*journal* journal 'put!) '(*state* network migrated) #t)
        :expect #t)
       (test-submit ((*journal* journal 'step!)) :expect integer?))
     (list journal-1 journal-2 journal-3))
    (test-report)
    (test-submit ((*journal* journal-2 'bridge!) journal-3) :expect #t)
    (test-report)
    (test-submit ((*journal* journal-2 'step!)) :expect integer?)
    (test-submit ((*journal* journal-1 'bridge!) journal-2) :expect #t)
    (test-report)
    (test-submit ((*journal* journal-1 'step!)) :expect integer?)
    (test-report)
    (test-submit ((*journal* journal-3 'step!)) :expect integer?)
    (test-submit ((*journal* journal-2 'bridge!) journal-3) :expect #t)
    (test-report)
    (test-submit ((*journal* journal-2 'step!)) :expect integer?)
    (test-submit ((*journal* journal-1 'bridge!) journal-2) :expect #t)
    (test-report)
    (test-submit ((*journal* journal-1 'step!)) :expect integer?)
    (test-report)

    (test-submit
     ((alice journal-1 journal-2 journal-3 'use!) '(*state* network seed))
     :expect 'journal-3)
    (test-submit
     ((alice journal-1 journal-2 journal-1 'use!) '(*state* network seed))
     :expect 'journal-1)

    (test-report)))
