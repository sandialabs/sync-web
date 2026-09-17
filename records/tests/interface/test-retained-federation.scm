(lambda (make-interface-harness)

  (with-let (make-interface-harness :journals 4 :users '(alice provider bob deep)
                                    :legacy? #t)

    (define (error-result? result)
      (and (pair? result) (eq? (car result) 'error)))

    (define (provider-request function arguments)
      (raw
       journal-1
       `((function ,function)
         (arguments ,arguments)
         (invocation
          ((identity alice)
           (route-source ())
           (route-target (journal-2))
           (credentials ,(journal-1 'credentials)))))))

    (define (unauthorized-provider-request function arguments)
      (raw
       journal-1
       `((function ,function)
         (arguments ,arguments)
         (invocation
          ((identity deep)
           (route-source ())
           (route-target (journal-2))
           (credentials ,(journal-1 'credentials)))))))

    (define alice-at-provider '(journal-1 *state* alice))
    (define provider-at-source '(journal-2 *state* bob))
    (define provider-through-middle '(journal-3 journal-2 *state* deep))

    (define (advertise-installed-inventory journal)
      (raw
       journal
       `(*call* ,(journal 'root-secret)
                (lambda (root)
                  (let ((class ((root 'get) '(root class chain))))
                    ((root 'set!) '(root class chain)
                     (append
                      class
                      '((define-method (indices self) '(chain () #f)))))
                    #t)))))

    ;; Resolve every `-1` from materialized permanent inventory, including a
    ;; nested Chain whose newer logical tail is an unavailable stub.
    (test-submit
     (raw
      journal-2
      `(*call* ,(journal-2 'root-secret)
               (lambda (root)
                 (let* ((standard (sync-eval ((root 'get) '(root object standard))))
                        (ledger (sync-eval ((root 'get) '(root object ledger))))
                        (federation
                         (sync-eval ((root 'get) '(root object federation))))
                        (tree-class ((root 'get) '(root class tree)))
                        (chain-class ((root 'get) '(root class chain)))
                        (inner (sync-eval ((standard 'init) chain-class)))
                        (inner-0 (sync-eval ((standard 'init) tree-class)))
                        (inner-1 (sync-eval ((standard 'init) tree-class)))
                        (outer (sync-eval ((standard 'init) chain-class)))
                        (empty (sync-eval ((standard 'init) chain-class)))
                        (tail-only
                         (sync-eval
                          ((standard 'init)
                           '(define-class (tail-only-chain)
                              (define-method (*init* self payload)
                                (set! (self '(1)) payload))
                              (define-method (size self) 800000)
                              (define-method (get self index)
                                (if (= index 799999) (self '(1))
                                    (error 'unexpected-get
                                           "Complete tail lookup read an earlier index"))))
                           (inner-0))))
                        (outer-0 (sync-eval ((standard 'init) tree-class)))
                        (outer-1 (sync-eval ((standard 'init) tree-class)))
                        (path '(-1 archive -1 *state* alice value))
                        (explicit '(0 archive 0 *state* alice value)))
                   ((inner-0 'set!) '(*state* alice value) "permanent")
                   ((inner-1 'set!) '(*state* alice value) "unavailable")
                   ((inner 'push!) (inner-0))
                   ((inner 'push!) (inner-1))
                   ((inner 'slice!) 0)
                   ((outer-0 'set!) '(*bridge* archive chain) (inner))
                   ((outer 'push!) (outer-0))
                   ((outer 'push!) (outer-1))
                   ((outer 'slice!) 0)
                   (let ((digest (sync-digest (outer))))
                     (list
                      ((ledger '~resolve-permanent-path)
                       (outer)
                       '(-1 (*bridge* archive chain) -1 (*state* alice value)))
                      ((ledger '~resolve-permanent-path)
                       (outer)
                       '(0 (*bridge* archive chain) 0 (*state* alice value)))
                      ((ledger '~resolve-permanent-path) (tail-only) '(-1))
                      (equal? digest (sync-digest (outer)))
                      (catch #t
                        (lambda ()
                          ((ledger '~resolve-permanent-path) (empty) '(-1)))
                        (lambda args (car args)))
                      (let* ((arguments
                              `((path ,path) (expression? #t)))
                             (response
                              `((content "permanent")
                                (proof ,((standard 'serialize) (outer))))))
                        (equal?
                         ((federation '~verify-retrieve-response)
                          ledger (outer) arguments response #t)
                         response))
                      (let* ((arguments
                              `((paths (,path ,explicit))
                                (expression? #t)))
                             (response
                              `((results
                                 (((path ,path) (content "permanent"))
                                  ((path ,explicit) (content "permanent"))))
                                (proof ,((standard 'serialize) (outer))))))
                        (equal?
                         ((federation '~verify-retrieve-batch-response)
                          ledger (outer) arguments response #t)
                         response))
                      (catch #t
                        (lambda ()
                          ((federation '~verify-retrieve-response)
                           ledger (outer)
                           `((path ,path) (expression? #t))
                           `((content "tampered")
                             (proof ,((standard 'serialize) (outer))))
                           #t))
                        (lambda args (car args)))
                      (catch #t
                        (lambda ()
                          ((federation '~verify-retrieve-batch-response)
                           ledger (outer)
                           `((paths (,path)) (expression? #t))
                           `((results
                              (((path ,path) (content "tampered"))))
                             (proof ,((standard 'serialize) (outer))))
                           #t))
                        (lambda args (car args)))
                      (catch #t
                        (lambda ()
                          ((federation '~verify-retrieve-response)
                           ledger (empty)
                           '((path (-1)) (expression? #t))
                           `((content unavailable)
                             (proof ,((standard 'serialize) (empty))))
                           #t))
                        (lambda args (car args)))
                      (catch #t
                        (lambda ()
                          ((federation '~verify-retrieve-response)
                           ledger (outer)
                           '((path (-1)) (expression? #t))
                           `((content unavailable)
                             (proof ,((standard 'serialize) (empty))))
                           #t))
                        (lambda args (car args)))))))))
     :expect '((0 (*bridge* archive chain) 0 (*state* alice value))
               (0 (*bridge* archive chain) 0 (*state* alice value))
               (799999) #t availability-error #t #t
               integrity-error integrity-error availability-error
               integrity-error))

    ;; Historical Chain objects intentionally lack `indices`; the installed
    ;; class advertises a different result so fallback cannot substitute it.
    (for-each
     (lambda (journal)
       (test-submit (advertise-installed-inventory journal) :expect #t))
     (list journal-1 journal-2 journal-3 journal-4))

    ;; Commit local state at every journal. Journal 2's index-0 value will later
    ;; remain available only through temp, while Journal 3 is the attributed source.
    (test-submit ((*journal* journal-1 'put!) '(*state* alice seed) "requester") :expect #t)
    (test-submit ((*journal* journal-2 'put!) '(*state* provider seed) "responder") :expect #t)
    (test-submit ((*journal* journal-2 'put!) '(*state* temporary value) "temp-only") :expect #t)
    (test-submit ((*journal* journal-3 'put!) '(*state* bob document) "retained") :expect #t)
    (test-submit ((*journal* journal-4 'put!) '(*state* deep document) "multi-hop") :expect #t)
    (test-submit
     ((*journal* journal-3 'authorize!)
      `((user (*state* bob))
        (rule ((principal ,provider-at-source) (key-index (-20 -1))
               (path (document)) (use! ((read-only? #t)))
               (put! #f) (retrieve (-20 -1))))))
     :expect #t)
    (test-submit
     ((*journal* journal-2 'authorize!)
      `((user (*state* provider))
        (rule ((principal ,alice-at-provider) (key-index (-20 -1))
               (path (seed)) (use! ((read-only? #t)))
               (put! #f) (retrieve (-20 -1))))))
     :expect #t)
    (test-submit
     ((*journal* journal-2 'authorize!)
      `((user (*state* bob))
        (rule ((principal ,alice-at-provider) (key-index (-20 -1))
               (path (document)) (use! ((read-only? #t)))
               (put! #f) (retrieve (-20 -1))))))
     :expect #t)
    (test-submit
     ((*journal* journal-2 'authorize!)
      `((user (*state* deep))
        (rule ((principal ,alice-at-provider) (key-index (-20 -1))
               (path (document)) (use! ((read-only? #t)))
               (put! #f) (retrieve (-20 -1))))))
     :expect #t)
    (test-submit
     ((*journal* journal-4 'authorize!)
      `((user (*state* deep))
        (rule ((principal ,provider-through-middle) (key-index (-20 -1))
               (path (document)) (use! ((read-only? #t)))
               (put! #f) (retrieve (-20 -1))))))
     :expect #t)
    (test-submit
     ((*journal* journal-2 'authorize!)
      `((user (*state* temporary))
        (rule ((principal ,alice-at-provider) (key-index (-20 -1))
               (path (value)) (use! ((read-only? #t)))
               (put! #f) (retrieve (-20 -1))))))
     :expect #t)
    (for-each
     (lambda (journal)
       (test-submit ((*journal* journal 'step!)) :expect 1))
     (list journal-1 journal-2 journal-3 journal-4))
    (test-report)

    ;; Commit the reverse route first, then propagate Journal 4 through Journal 3
    ;; into Journal 2 exactly as a normal two-hop federation route.
    (test-submit ((*journal* journal-2 'bridge!) journal-3)
                 :schedule '(2 1) :expect #t)
    (test-report)
    (test-submit ((*journal* journal-3 'step!)) :expect 2)
    (test-submit ((*journal* journal-3 'bridge!) journal-4)
                 :schedule '(1 2) :tick 1 :expect #t)
    (test-report)
    (test-submit ((*journal* journal-4 'step!)) :expect 2)
    (test-submit ((*journal* journal-3 'step!)) :expect 3)
    (test-report)
    (test-submit ((*journal* journal-3 'bridge!) journal-4)
                 :schedule '(2 1) :tick 1 :expect #t)
    (test-report)
    (test-submit ((*journal* journal-3 'step!)) :expect 3)
    (test-submit ((*journal* journal-2 'bridge!) journal-3)
                 :schedule '(1 2) :tick 1 :expect #t)
    (test-report)
    (test-submit ((*journal* journal-2 'step!)) :expect 2)
    (test-submit
     ((bob journal-2 journal-3 'pin! :history '(1 0))
      '(-1 *state* bob document))
     :schedule '(2 1 1 0) :tick 1 :expect #t)
    (test-submit
     ((bob journal-2 journal-3 'pin! :history '(1 2))
      '(-1 *state* bob document))
     :schedule '(2 1 1 0) :tick 1 :expect #t)
    (test-submit
     ((deep journal-2 journal-3 journal-4 'pin! :history '(1 2 0))
      '(-1 *state* deep document))
     :schedule '(2 1 2 0 1 0) :tick 1 :expect #t)
    (test-report)

    ;; Remove index 0 only from permanent evidence. Its recent temp copy remains.
    (test-submit ((*journal* journal-2 'unpin!) '(0 *state* temporary value))
                 :expect #t)
    (test-submit
     ((*journal* journal-2 'retrieve)
      '(0 *state* temporary value) :pinned? #t :proof? #f)
     :expect '((content "temp-only") (pinned? #f)))
    (test-report)

    ;; Establish the explicit requester -> responder route after retention is set.
    (test-submit ((*journal* journal-1 'bridge!) journal-2)
                 :schedule '(2 1) :expect #t)
    (test-report)
    (test-submit
     ((*journal* journal-2 'put!) '(*state* provider seed) "newer-provider")
     :expect #t)
    (test-submit ((*journal* journal-2 'step!)) :expect 3)
    (test-submit ((*journal* journal-1 'bridge!) journal-2)
                 :schedule '(1 2) :tick 1 :expect #t)
    (test-report)
    (test-submit ((*journal* journal-1 'step!)) :expect 2)
    (test-report)

    ;; The unavailable historical-bridge subtype is stable and message-independent.
    (test-submit
     (raw
      journal-1
      `(*call* ,(journal-1 'root-secret)
               (lambda (root)
                 (let ((ledger
                        (sync-eval ((root 'get) '(root object ledger)))))
                   (catch #t
                     (lambda () ((ledger 'peer-head) 'journal-2 0))
                     (lambda args (car args)))))))
     :expect 'bridge-index-error)
    (test-report)

    (test-submit
     ((*journal* journal-2 'pin!) '(1 *state* provider seed))
     :expect #t)
    (test-report)

    ;; Keep the requester's authenticated provider head rich while making the
    ;; responder's newest provider root a digest-preserving permanent stub.
    ;; Origin selection must be derived from the returned permanent proof, not
    ;; from extra same-root material in its local route head.
    (test-submit
     (raw
      journal-2
      `(*call* ,(journal-2 'root-secret)
               (lambda (root)
                 (let* ((standard
                         (sync-eval ((root 'get) '(root object standard))))
                        (ledger
                         (sync-eval ((root 'get) '(root object ledger))))
                        (perm ((ledger '~field!) 'perm))
                        (digest (sync-digest perm))
                        (cut ((standard 'deep-prune!) perm '(2))))
                   ((ledger '~field!) 'perm cut)
                   ((root 'set!) '(root object ledger) (ledger))
                   (equal? digest (sync-digest cut))))))
     :expect #t)
    (test-report)
    (test-submit
     (raw
      journal-2
      `(*call* ,(journal-2 'root-secret)
               (lambda (root)
                 (let* ((ledger
                         (sync-eval ((root 'get) '(root object ledger))))
                        (perm ((ledger '~field!) 'perm))
                        (source ((ledger '~child) perm 'previous 2))
                        (single
                         ((ledger 'read) 2 source
                          '((paths ((-1 (*state* provider seed))))
                            (resolve-latest-perm? #t))))
                        (duplicate
                         ((ledger 'read) 2 source
                          '((paths ((-1 (*state* provider seed))
                                    (1 (*state* provider seed))))
                            (resolve-latest-perm? #t)))))
                   (and (equal? (sync-digest (cadr (assoc 'object single)))
                                (sync-digest (cadr (assoc 'object duplicate))))
                        (equal? (cadr (assoc 'paths duplicate))
                                '((1 (*state* provider seed))
                                  (1 (*state* provider seed)))))))))
     :expect #t)
    (test-report)
    (test-submit
     (provider-request
      'retrieve
      '((path (-1 *state* provider seed))
        (pinned? #f) (proof? #f) (index? #t) (expression? #t)))
     :schedule '(1 2)
     :expect '((content "responder") (indexes (1))))
    (test-submit
     (provider-request
      'retrieve-batch
      '((paths ((-1 *state* provider seed)
                (1 *state* provider seed)))
        (pinned? #f) (proof? #f) (index? #t) (expression? #t)))
     :schedule '(1 2)
     :expect
     '((results
        (((path (-1 *state* provider seed))
          (content "responder") (indexes (1)))
         ((path (1 *state* provider seed))
          (content "responder") (indexes (1)))))))
    (test-report)

    ;; The explicit route ends at Journal 2. Journal 3 is not contacted while
    ;; Journal 2 evaluates its permanently retained path and returns a proof.
    (test-submit
     (provider-request
      'retrieve
      '((path (1 journal-3 0 *state* bob document))
        (pinned? #f) (proof? #f) (index? #t) (expression? #t)))
     :schedule '(1 2)
     :expect '((content "retained") (indexes (1 0))))
    (test-submit
     (provider-request
      'retrieve
      '((path (1 journal-3 0 *state* bob document))
        (pinned? #t) (proof? #f) (expression? #t)))
     :schedule '(1 2)
     :expect '((content "retained") (pinned? #f)))
    (test-submit
     (provider-request
      'retrieve
      '((path (1 journal-3 *state* bob document))
        (pinned? #f) (proof? #f) (expression? #t)))
     :schedule '(1 2) :expect "retained")
    (test-submit
     (provider-request
      'retrieve
      '((path (1 journal-3 2 journal-4 0 *state* deep document))
        (pinned? #f) (proof? #f) (index? #t) (expression? #t)))
     :schedule '(1 2)
     :expect '((content "multi-hop") (indexes (1 2 0))))
    (test-submit
     (provider-request
      'retrieve
      '((path (1 *bridge* journal-3 0 *state* bob document))
        (pinned? #f) (proof? #f) (expression? #t)))
     :schedule '(1 2) :expect "retained")
    (test-report)

    ;; Bridge discovery and trailing-alias inventory both come from responder perm.
    (test-submit
     (provider-request
      'retrieve
      '((path (1 *bridge*)) (pinned? #f) (proof? #f) (expression? #t)))
     :schedule '(1 2)
     :expect (lambda (result)
               (and (pair? result) (eq? (car result) 'directory)
                    (assoc 'journal-3 (cadr result)))))
    (test-submit
     (provider-request
      'retrieve
      '((path (1 journal-3)) (pinned? #f) (proof? #f) (expression? #t)))
     :schedule '(1 2)
     :expect (lambda (result)
               (and (list? result) (= (length result) 3)
                    (eq? (car result) 'chain)
                    (list? (cadr result)) (member 0 (cadr result))
                    (boolean? (caddr result)))))
    (test-submit
     (provider-request
      'retrieve
      '((path (1 journal-3 0 *state*))
        (pinned? #f) (proof? #f) (expression? #t)))
     :schedule '(1 2)
     :expect (lambda (result) (and (pair? result) (eq? (car result) 'directory))))
    (test-submit
     (unauthorized-provider-request
      'retrieve
      '((path (1 journal-3 0 *state*))
        (pinned? #f) (proof? #f) (expression? #t)))
     :schedule '(1 2) :expect error-result?)
    (test-report)

    (test-submit
     (provider-request
      'retrieve-batch
      '((paths ((1 journal-3 0 *state* bob document)
                (1 journal-3 *state* bob document)))
        (pinned? #f) (proof? #f) (index? #t) (expression? #t)))
     :schedule '(1 2)
     :expect
     '((results
        (((path (1 journal-3 0 *state* bob document))
          (content "retained") (indexes (1 0)))
         ((path (1 journal-3 *state* bob document))
          (content "retained") (indexes (1 2)))))))
    (test-report)

    ;; Temp-only and unauthorized retained requests stop at the chosen responder.
    (test-submit
     (provider-request
      'retrieve
      '((path (0 *state* temporary value))
        (pinned? #f) (proof? #f) (expression? #t)))
     :schedule '(1 2) :expect error-result?)
    (test-submit
     (provider-request
      'retrieve
      '((path (1 journal-3 0 *state* bob missing))
        (pinned? #f) (proof? #f) (expression? #t)))
     :schedule '(1 2) :expect error-result?)
    (test-report)))
