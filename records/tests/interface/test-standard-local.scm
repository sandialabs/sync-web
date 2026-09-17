(lambda (make-interface-harness)

  ;; The fixture's embedded Chain maps -7 to index 0 and probes one host-only
  ;; primitive at -8. The installed class intentionally differs at both values.
  (with-let (make-interface-harness :journals 2 :journal-start 31
                                    :users '(alice) :legacy? #t)

    (define (restore-installed-chain journal)
      (raw
       journal
       `(*call* ,(journal 'root-secret)
                (lambda (root)
                  (let* ((class ((root 'get) '(root class chain)))
                         (replacement
                          '(define-method (index self index~)
                             (case index~
                               ((-8) 0)
                               (else ((self '~adjust) index~)))))
                         (installed
                          (cons
                           'define-class
                           (cons
                            (cadr class)
                            (map
                             (lambda (form)
                               (if (and (pair? form)
                                        (eq? (car form) 'define-method)
                                        (pair? (cdr form))
                                        (pair? (cadr form))
                                        (eq? (caadr form) 'index))
                                   replacement form))
                             (cddr class))))))
                    ((root 'set!) '(root class chain) installed)
                    #t)))))

    (define (error-result? result)
      (and (pair? result) (eq? (car result) 'error)))

    (define* (collect action (schedule '()))
      (test-report)
      (test-submit action :schedule schedule)
      (test-await))

    (define (fingerprint journal)
      (collect
       (raw journal
            `(*call* ,(journal 'root-secret)
                     (lambda (root)
                       (list (sync-digest (root))
                             (sync-digest
                              ((root 'get) '(root object ledger)))))))))

    (test-submit (restore-installed-chain journal-31) :expect #t)
    (test-submit (restore-installed-chain journal-32) :expect #t)
    (test-submit ((*journal* journal-31 'put!) '(*state* alice seed) "origin") :expect #t)
    (test-submit ((*journal* journal-32 'put!) '(*state* remote value) "embedded-0") :expect #t)
    (test-submit
     ((*journal* journal-32 'authorize!)
      '((user (*state* remote))
        (rule ((principal (journal-31 *state* alice))
               (key-index (-10 -1)) (path (value))
               (use! ((read-only? #t))) (put! #f) (retrieve #t)))))
     :expect #t)
    (test-submit ((*journal* journal-31 'step!)) :expect 1)
    (test-submit ((*journal* journal-32 'step!)) :expect 1)
    (test-submit ((*journal* journal-32 'put!) '(*state* remote value) "embedded-1") :expect #t)
    (test-submit ((*journal* journal-32 'step!)) :expect 2)
    (test-report)

    (test-submit ((*journal* journal-31 'bridge!) journal-32) :schedule '(2 1) :expect #t)
    (test-report)
    (test-submit ((*journal* journal-32 'step!)) :expect 3)
    (test-submit ((*journal* journal-31 'bridge!) journal-32)
                 :schedule '(1 2) :tick 1 :expect #t)
    (test-report)
    (test-submit ((*journal* journal-31 'step!)) :expect 2)
    (test-report)

    ;; Only the authenticated proof's embedded index method maps -7 to 0.
    (test-submit
     ((alice journal-31 journal-32 'retrieve :history '(-1 -7))
      '(-1 *state* remote value) :pinned? #f :proof? #f :index? #t)
     :schedule '(1 2 1 0)
     :expect '((content "embedded-0") (indexes (1 0))))
    (let* ((plain
            (collect
             ((alice journal-31 journal-32 'retrieve :history '(-1 -7))
              '(-1 *state* remote value) :pinned? #f :proof? #t)
             '(1 2 1 0)))
           (indexed
            (collect
             ((alice journal-31 journal-32 'retrieve :history '(-1 -7))
              '(-1 *state* remote value) :pinned? #f :proof? #t :index? #t)
             '(1 2 1 0))))
      (if (not (equal? (cadr (assoc 'proof plain))
                       (cadr (assoc 'proof indexed))))
          (error 'proof-error "Embedded index projection changed proof bytes")))
    (test-report)

    ;; First authenticate and return the proof through the ordinary federated
    ;; path. Its embedded host-call probe must then fail inside Standard even
    ;; though the requester's installed class accepts the same sentinel.
    (let* ((authenticated
            (collect
             ((alice journal-31 journal-32 'retrieve :history '(-1 -1))
              '(-1 *state* remote value) :pinned? #f :proof? #t)
             '(1 2 1 0)))
           (proof (cadr (assoc 'proof authenticated)))
           (before (map fingerprint (list journal-31 journal-32))))
      (test-submit
       (raw
        journal-31
        `(*call* ,(journal-31 'root-secret)
                 (lambda (root)
                   (let* ((module
                           (eval ((root 'get) '(root class standard-module))))
                          (standard
                           (module 'local
                                   ((root 'get) '(root class standard))
                                   ((root 'get) '(root object standard))))
                          (chain ((standard 'deserialize) ',proof)))
                     ((standard 'deep-call) chain '()
                      '(lambda (object) ((object 'index) -8)))))))
       :expect error-result?)
      (test-report)
      (if (not (equal? before (map fingerprint (list journal-31 journal-32))))
          (error 'state-error "Contained proof-code failure changed Journal state")))
    (test-report)))
