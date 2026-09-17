(lambda (make-interface-harness)

  (with-let (make-interface-harness :journals 2 :users '(alice bob)
                                    :admins '((*state* bob)) :window #f)

    (define (error-result? result)
      (and (list? result) (eq? (car result) 'error)))

    (define (collect action)
      (test-report)
      (test-submit action)
      (test-await))

    ;; Build committed history, retain explicit evidence in an old head, and
    ;; leave a newer staged value that truncation must not change.
    (for-each
     (lambda (version)
       (test-submit
        ((*journal* journal-1 'put!) '(*state* alice document) version)
        :expect #t)
       (test-submit ((*journal* journal-1 'step!)) :expect (+ version 1)))
     '(0 1 2 3))
    (test-submit ((*journal* journal-1 'pin!) '(0 *state* alice document))
                 :expect #t)
    (test-submit
     ((*journal* journal-1 'retrieve)
      '(0 *state* alice document) :pinned? #t :proof? #f)
     :expect '((content 0) (pinned? #t)))
    (test-submit
     ((*journal* journal-1 'put!) '(*state* alice document) "staged")
     :expect #t)
    (test-submit
     ((*journal* journal-1 'put!) '(*state* alice uncommitted) "keep")
     :expect #t)
    (test-report)

    (let ((salt (collect ((*journal* journal-1 'config)
                          :path '(public key-derivation-salt))))
          (size (collect ((*journal* journal-1 'size)))))
      ;; Namespace ownership and ordinary grants never confer this local
      ;; administrative operation. The configured local administrator succeeds.
      (test-submit
       ((alice journal-1 'authorize!)
        '((user (*state* alice))
          (rule ((principal (*state* alice)) (path ())
                 (use! ((read-only? #t))) (put! #t) (retrieve #t)))))
       :expect #t)
      (test-submit ((alice journal-1 'truncate!) :index 1)
                   :expect error-result?)
      (test-submit ((*anonymous* journal-1 'truncate!) :index 1)
                   :expect error-result?)
      (test-submit ((bob journal-1 'truncate!) :index 1) :expect #t)
      (test-submit ((*journal* journal-1 'size)) :expect size)
      (test-submit
       ((*journal* journal-1 'config) :path '(public key-derivation-salt))
       :expect salt))

    ;; Both permanent and temporary history lose the inclusive prefix. Explicit
    ;; pinning cannot preserve content through an administrative truncation.
    (test-submit
     (raw journal-1
          '(*call* "pass-1"
                   (lambda (root)
                     (let ((ledger
                            (sync-eval ((root 'get) '(root object ledger)))))
                       (map
                        (lambda (field)
                          (let ((chain (sync-eval ((ledger '~field!) field))))
                            (list ((chain 'size))
                                  ((chain 'get) 0)
                                  ((chain 'get) 1)
                                  (sync-node? ((chain 'get) 2))
                                  (sync-node? ((chain 'get) 3)))))
                        '(perm temp))))))
     :expect '((4 (unknown) (unknown) #t #t)
               (4 (unknown) (unknown) #t #t)))
    (test-submit
     ((*journal* journal-1 'retrieve)
      '(0 *state* alice document) :pinned? #t :proof? #f)
     :expect '(unknown))
    (test-submit
     ((*journal* journal-1 'retrieve)
      '(1 *state* alice document) :pinned? #f :proof? #f)
     :expect '(unknown))
    (test-submit
     ((*journal* journal-1 'retrieve)
      '(2 *state* alice document) :pinned? #f :proof? #f)
     :expect 2)
    (test-submit ((*journal* journal-1 'use!) '(*state* alice document))
                 :expect "staged")
    (test-submit ((*journal* journal-1 'use!) '(*state* alice uncommitted))
                 :expect "keep")

    ;; A failure in the second chain candidate leaves both Ledger fields exact.
    (test-submit
     (raw journal-1
          '(*call* "pass-1"
                   (lambda (root)
                     (let* ((ledger
                             (sync-eval ((root 'get) '(root object ledger))))
                            (original-temp ((ledger '~field!) 'temp))
                            (malformed-temp
                             (sync-cons
                              (sync-car original-temp)
                              (sync-cons
                               (sync-car (sync-cdr original-temp))
                               (sync-cut (sync-cdr (sync-cdr original-temp)))))))
                       ((ledger '~field!) 'temp malformed-temp)
                       (let* ((perm-before
                               (sync-digest ((ledger '~field!) 'perm)))
                              (temp-before
                               (sync-digest ((ledger '~field!) 'temp)))
                              (failed
                               (catch #t
                                 (lambda ()
                                   ((ledger 'truncate!) 2)
                                   #f)
                                 (lambda args #t)))
                              (unchanged
                               (and
                                (equal? perm-before
                                        (sync-digest ((ledger '~field!) 'perm)))
                                (equal? temp-before
                                        (sync-digest ((ledger '~field!) 'temp))))))
                         ((ledger '~field!) 'temp original-temp)
                         ((root 'set!) '(root object ledger) (ledger))
                         (and failed unchanged))))))
     :expect #t)

    ;; Latest truncation preserves staged state and numbering; a later commit
    ;; appends at the original next index. Lower cutoffs cannot resurrect data.
    (test-submit ((*journal* journal-1 'step!)) :expect 5)
    (test-submit ((*journal* journal-1 'truncate!) :index -1) :expect #t)
    (test-submit
     ((*journal* journal-1 'retrieve)
      '(4 *state* alice document) :pinned? #f :proof? #f)
     :expect '(unknown))
    (test-submit ((*journal* journal-1 'use!) '(*state* alice document))
                 :expect "staged")
    (test-submit ((*journal* journal-1 'truncate!) :index 3) :expect #t)
    (test-submit
     ((*journal* journal-1 'retrieve)
      '(4 *state* alice document) :pinned? #f :proof? #f)
     :expect '(unknown))
    (test-submit
     ((*journal* journal-1 'put!) '(*state* alice document) "after")
     :expect #t)
    (test-submit ((*journal* journal-1 'step!)) :expect 6)
    (test-submit
     ((*journal* journal-1 'retrieve)
      '(5 *state* alice document) :pinned? #f :proof? #f)
     :expect "after")

    ;; Invalid indexes preserve state. A routed form is not in the federation
    ;; allowlist and cannot turn a remote principal into an administrator.
    (test-submit ((*journal* journal-1 'truncate!) :index 6)
                 :expect error-result?)
    (test-submit ((*journal* journal-1 'truncate!) :index -7)
                 :expect error-result?)
    (test-submit ((bob journal-2 journal-1 'truncate!) :index 5)
                 :expect error-result?)

    (test-report)))
