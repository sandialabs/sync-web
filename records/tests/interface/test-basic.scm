(lambda (make-interface-harness)

  (with-let (make-interface-harness :journals 7 :users '(alice bob carol) :tick 1)

  (define (collect submitted-action)
    (test-report)
    (test-submit submitted-action)
    (test-await))

  (define (share journal user principal path get resolve)
    ((apply *journal* (list journal 'authorize!))
     `((user ,user)
       (rule ((principal ,principal)
              ,@(if (and (pair? principal)
                         (not (memq (car principal) '(*public* *state*))))
                    '((key-index (0 -1))) '())
              (path ,path) (get ,get) (set! #f) (resolve ,resolve))))))

  (test-submit ((*journal* journal-1 'size)) :tick 3 :expect 0)

  ;; Fresh installs contain no Document class, field, or method surface.
  (test-submit
    (raw journal-1
         '(*call* "pass-1"
                  (lambda (root)
                    (let ((ledger
                           (sync-eval
                            ((root 'get) '(root object ledger)))))
                      (list ((root 'get) '(root class document))
                            (member 'document (ledger '*api*))
                            (member 'batch! (ledger '*api*))
                            (member 'copy (ledger '*api*))
                            (byte-vector? (sync-car (ledger '(1 1)))))))))
    :expect '((nothing) #f #f #f #t))
  (test-submit ((*journal* journal-1 'batch!) '())
               :expect (lambda (result)
                         (and (list? result) (eq? (car result) 'error))))
  (test-submit ((*journal* journal-1 'copy) '())
               :expect (lambda (result)
                         (and (list? result) (eq? (car result) 'error))))

  ;; Reinstallation is rejected before changing root/state/code fingerprints.
  (let ((fingerprint
         (collect
          (raw journal-1
               '(*call* "pass-1"
                        (lambda (root)
                          (let ((ledger
                                 (sync-eval
                                  ((root 'get) '(root object ledger)))))
                            (list (sync-digest (root))
                                  (sync-digest ((root 'get)
                                                '(root object federation)))
                                  (sync-digest ((root 'get)
                                                '(root object authorization)))
                                  (sync-digest ((root 'get)
                                                '(root class federation)))
                                  (sync-digest (ledger))
                                  (sync-digest (sync-car (ledger)))
                                  (sync-digest ((ledger '~field!) 'stage))
                                  (sync-digest ((ledger '~field!) 'perm))))))))))
    (test-submit (update-interface journal-1 '() 4 #t)
      :expect (lambda (result)
                (and (list? result) (eq? (car result) 'error))))
    (test-submit
      (raw journal-1
           '(*call* "pass-1"
                    (lambda (root)
                      (let ((ledger
                             (sync-eval
                              ((root 'get) '(root object ledger)))))
                        (list (sync-digest (root))
                              (sync-digest ((root 'get)
                                            '(root object federation)))
                              (sync-digest ((root 'get)
                                            '(root object authorization)))
                              (sync-digest ((root 'get)
                                            '(root class federation)))
                              (sync-digest (ledger))
                              (sync-digest (sync-car (ledger)))
                              (sync-digest ((ledger '~field!) 'stage))
                              (sync-digest ((ledger '~field!) 'perm)))))))
      :expect fingerprint))

  ;; Arbitrary structured Tree values stay opaque during Ledger reads.
  (test-submit
    (raw journal-1
         '(*call* "pass-1"
                  (lambda (root)
                    (let* ((ledger
                            (sync-eval
                             ((root 'get) '(root object ledger))))
                           (stage
                            (sync-eval ((ledger '~field!) 'stage)))
                           (opaque
                            (sync-cons
                             (expression->byte-vector
                              '(lambda (state)
                                 (error 'opaque-executed
                                        "Opaque value was executed")))
                             (sync-null))))
                      ((stage 'set!) '(*state* opaque) opaque)
                      ((ledger '~field!) 'stage (stage))
                      ((root 'set!) '(root object ledger) (ledger))
                      (sync-node? ((ledger 'get) '(*state* opaque)))))))
    :expect #t)

  (test-submit ((*journal* journal-1 'set!) '(*state* hello) "world") :expect #t)
  (test-submit
    ((*journal* journal-1 'set!) '(*state* hello) "wrong"
     :expected "not-world")
    :expect #f)
  (test-submit ((*journal* journal-1 'get) '(*state* hello)) :expect "world")
  (test-submit
    ((*journal* journal-1 'get) '(*transition* operation) :expression? #f)
    :expect '((path (*state* hello))
              (value #u(34 119 111 114 108 100 34))))
  (test-submit
    ((*journal* journal-1 'set!) '(*state* hello) "updated"
     :expected "world")
    :expect #t)
  (test-submit
    ((*journal* journal-1 'set!) '(*state* hello) "world"
     :expected "updated")
    :expect #t)
  (test-submit ((*journal* journal-1 'set!) '(*state* conditional false) #f)
               :expect #t)
  (test-submit
    ((*journal* journal-1 'set!) '(*state* conditional false) #t
     :expected #f)
    :expect #t)
  (test-submit
    ((*journal* journal-1 'set!) '(*state* conditional missing) "created"
     :expected '(nothing))
    :expect #t)

  ;; Equal-tick contenders all compare with the same expected value. Journal
  ;; collision retries re-evaluate against the winner's root, so exactly one
  ;; succeeds and every other request returns #f without another transition.
  (test-submit
    ((*journal* journal-1 'set!) '(*state* conditional contention) "old")
    :expect #t)
  (test-report)
  (let ((contenders
         (map
          (lambda (value)
            (test-submit
             ((*journal* journal-1 'set!)
              '(*state* conditional contention) value :expected "old")
             :expect boolean?))
          '(winner-0 winner-1 winner-2 winner-3 winner-4 winner-5 winner-6 winner-7))))
    (test-report)
    (let loop ((remaining contenders) (successes 0))
      (if (null? remaining)
          (if (not (= successes 1))
              (error 'contention-error
                     "Conditional writers produced ~S successes" successes))
          (loop (cdr remaining)
                (if ((car remaining)) (+ successes 1) successes))))
    (test-submit
      ((*journal* journal-1 'get) '(*state* conditional contention))
      :expect (lambda (value)
                (memq value
                      '(winner-0 winner-1 winner-2 winner-3
                        winner-4 winner-5 winner-6 winner-7)))))

  (test-submit ((*journal* journal-1 'get) :path '(*state* hello)) :expect "world")
  ;; Public paths are flat; nested path objects are rejected at the Interface boundary.
  (test-submit
    ((*journal* journal-1 'get) '((*state* hello)))
    :expect (lambda (result) (and (list? result) (eq? (car result) 'error))))

  (test-submit ((*journal* journal-1 'set-batch!)
     '((paths ((*state* batch alpha) (*state* batch beta)))
       (values ("a" "b"))
       (expression? #t))) :expect #t)
  (test-submit ((*journal* journal-1 'get) '(*state* batch alpha)) :expect "a")
  (test-submit ((*journal* journal-1 'get) '(*state* batch beta)) :expect "b")
  (test-submit
    ((*journal* journal-1 'set-batch!)
     '((paths ((*state* batch alpha) (*state* batch beta)))
       (values ("wrong-a" "wrong-b"))
       (expected ("a" "not-b"))
       (expression? #t)))
    :expect #f)
  (test-submit
    ((*journal* journal-1 'get-batch)
     '((*state* batch alpha) (*state* batch beta)))
    :expect
    '((results
       (((path (*state* batch alpha)) (content "a"))
        ((path (*state* batch beta)) (content "b"))))))
  (test-submit
    ((*journal* journal-1 'set-batch!)
     '((paths ((*state* batch alpha) (*state* batch beta)))
       (values ("new-a" "new-b"))
       (expected ("a" "b"))
       (expression? #t)))
    :expect #t)
  (test-submit
    ((*journal* journal-1 'set-batch!)
     '((paths ((*state* batch alpha) (*state* batch beta)))
       (values ("a" "b"))
       (expected ("new-a" "new-b"))
       (expression? #t)))
    :expect #t)
  (test-submit
    ((*journal* journal-1 'set-batch!)
     '((paths ((*state* batch alpha) (*state* batch beta)))
       (values ("x" "y"))
       (expected ("a"))
       (expression? #t)))
    :expect (lambda (result)
              (and (list? result) (eq? (car result) 'error))))
  (test-submit
    ((*journal* journal-1 'get-batch)
     (make-list 1025 '(*state* hello)))
    :expect (lambda (result)
              (and (list? result) (eq? (car result) 'error))))
  (test-submit ((*journal* journal-1 'get-batch) '())
               :expect '((results ())))
  (test-submit
    ((*journal* journal-1 'set-batch!)
     '((paths ()) (values ()) (expected ()) (expression? #t)))
    :expect #t)
  (test-submit
    ((*journal* journal-1 'get-batch)
     '((*state* batch beta) (*state* hello) (*state* batch beta)))
    :expect
    '((results
       (((path (*state* batch beta)) (content "b"))
        ((path (*state* hello)) (content "world"))
        ((path (*state* batch beta)) (content "b"))))))
  (test-submit ((*journal* journal-1 'get) '(*transition* operation)
     :expression? #f)
    :expect '((path (*state* batch beta)) (value #u(34 98 34))))
  (test-submit ((*journal* journal-1 'get) '(*transition* previous operation)
     :expression? #f)
    :expect '((path (*state* batch alpha)) (value #u(34 97 34))))
  (test-submit ((*journal* journal-1 'get) '(*transition* previous previous operation)
     :expression? #f)
    :expect '((path (*state* batch beta))
              (value #u(34 110 101 119 45 98 34))))

  ;; Duplicate paths compare against one pre-write snapshot; successful writes
  ;; then retain ordinary input order, so the last replacement wins.
  (test-submit ((*journal* journal-1 'set!) '(*state* conditional duplicate) "old")
               :expect #t)
  (test-submit
    ((*journal* journal-1 'set-batch!)
     '((paths ((*state* conditional duplicate)
               (*state* conditional duplicate)))
       (values ("first" "second"))
       (expected ("old" "old"))
       (expression? #t)))
    :expect #t)
  (test-submit ((*journal* journal-1 'get) '(*state* conditional duplicate))
               :expect "second")
  (test-submit
    ((*journal* journal-1 'set-batch!)
     '((paths ((*state* conditional duplicate)
               (*state* conditional duplicate)))
       (values ("wrong" "also-wrong"))
       (expected ("second" "first"))
       (expression? #t)))
    :expect #f)
  (test-submit ((*journal* journal-1 'get) '(*state* conditional duplicate))
               :expect "second")

  (test-submit ((*journal* journal-1 'set!) '(*state* bytes doc) #u(4 5 6) :expression? #f) :expect #t)
  (test-submit ((*journal* journal-1 'get) '(*state* bytes doc) :expression? #f) :expect #u(4 5 6))
  (test-submit
    ((*journal* journal-1 'set!) '(*state* bytes doc) #u(7 8 9)
     :expected #u(0) :expression? #f)
    :expect #f)
  (test-submit
    ((*journal* journal-1 'set!) '(*state* bytes doc) #u(7 8 9)
     :expected #u(4 5 6) :expression? #f)
    :expect #t)
  (test-submit ((*journal* journal-1 'get) '(*state* bytes doc) :expression? #f)
               :expect #u(7 8 9))
  (test-submit
    ((*journal* journal-1 'set-batch!)
     '((paths ((*state* bytes batch) (*state* bytes absent)))
       (values (#u(10 11) #u(12)))
       (expected ((nothing) (nothing)))
       (expression? #f)))
    :expect #t)
  (test-submit
    ((*journal* journal-1 'set-batch!)
     '((paths ((*state* bytes batch) (*state* bytes absent)))
       (values (#u(13) (nothing)))
       (expected (#u(10 11) #u(12)))
       (expression? #f)))
    :expect #t)
  (test-submit
    ((*journal* journal-1 'get-batch)
     '((*state* bytes batch) (*state* bytes absent)) :expression? #f)
    :expect
    '((results
       (((path (*state* bytes batch)) (content #u(13)))
        ((path (*state* bytes absent)) (content (nothing)))))))
  (test-submit ((*journal* journal-1 'set!) '(*state* bytes rejected) "not bytes"
     :expression? #f) :expect (lambda (result) (and (list? result) (eq? (car result) 'error))))

  (test-submit ((*journal* journal-1 'step!)) :expect 1)

  ;; Public trace exposes a serialized committed proof and rejects unknown paths.
  (test-submit
    ((*journal* journal-1 'trace)
     '((index 0) (path (-1 *state* hello))))
    :expect (lambda (result)
              (and (list? result) (pair? result)
                   (let loop ((entries result))
                     (or (null? entries)
                         (and (pair? (car entries))
                              (= (length (car entries)) 2)
                              (loop (cdr entries))))))))
  (test-submit
    ((*journal* journal-1 'trace)
     '((index 0) (path (-1 *state* missing))))
    :expect (lambda (result)
              (and (list? result) (pair? result))))
  (test-submit
    ((*journal* journal-1 'trace)
     '((index 0) (path ((-1 *state* missing)))))
    :expect (lambda (result)
              (and (list? result) (eq? (car result) 'error))))

  (test-submit ((*journal* journal-1 'resolve) '(-1 *state* hello) :pinned? #f :proof? #f) :expect "world")
  (test-submit
    ((*journal* journal-1 'resolve-batch)
     '((-1 *state* hello) (-1 *state* batch alpha) (-1 *state* missing)))
    :expect
    '((results
       (((path (-1 *state* hello)) (content "world"))
        ((path (-1 *state* batch alpha)) (content "a"))
        ((path (-1 *state* missing)) (content (nothing)))))))
  (test-submit
    ((*journal* journal-1 'trace-batch)
     '((index 0)
       (paths ((-1 *state* hello) (-1 *state* batch alpha)))))
    :expect (lambda (result) (and (list? result) (pair? result))))
  ;; One trace-batch proof preserves the shared head, replays every requested
  ;; value, omits an unrequested sibling, and is smaller than two scalar traces.
  (test-submit
    (raw journal-1
         '(*call* "pass-1"
                  (lambda (root)
                    (let* ((standard
                            (sync-eval
                             ((root 'get) '(root object standard))))
                           (ledger
                            (sync-eval
                             ((root 'get) '(root object ledger))))
                           (batch
                            ((ledger 'trace-batch)
                             '((0 *state* hello)
                               (0 *state* batch alpha))))
                           (one ((ledger 'trace) '(0 *state* hello)))
                           (two ((ledger 'trace) '(0 *state* batch alpha)))
                           (proof ((standard 'deserialize) batch)))
                      (list
                       (equal? (sync-digest proof)
                               (sync-digest
                                ((standard 'deserialize) one)))
                       (byte-vector->expression
                        ((standard 'deep-get)
                         proof '(0 (*state* hello))))
                       (byte-vector->expression
                        ((standard 'deep-get)
                         proof '(0 (*state* batch alpha))))
                       ((standard 'deep-get)
                        proof '(0 (*state* batch beta)))
                       (< (length (expression->byte-vector batch))
                          (+ (length (expression->byte-vector one))
                             (length (expression->byte-vector two)))))))))
    :expect '(#t "world" "a" (unknown) #t))
  (test-submit
    (raw journal-1
         '(*call* "pass-1"
                  (lambda (root)
                    (let* ((ledger
                            (sync-eval
                             ((root 'get) '(root object ledger))))
                           (federation
                            (sync-eval
                             ((root 'get) '(root object federation))))
                           (head ((ledger '~head) '(0 (*state* hello))))
                           (proof
                            ((ledger 'trace-batch)
                             '((0 *state* hello)
                               (0 *state* batch alpha)))))
                      (catch #t
                        (lambda ()
                          ((federation '~verify-resolve-batch-response)
                           ledger head
                           '((paths ((0 *state* hello)
                                     (0 *state* batch alpha)))
                             (expression? #t))
                           `((results (((content "world"))))
                             (proof ,proof))))
                        (lambda (tag . rest) tag))))))
    :expect 'integrity-error)
  ;; Prepared group tables are internal and reject malformed/out-of-range or
  ;; unreferenced slots before any retention mutation.
  (for-each
   (lambda (prepared)
     (test-submit
      (raw journal-1
           `((function pin-batch!)
             (arguments ((paths ((-1 *state* hello)))))
             (authentication
              ((identity (*state* admin))
               (credentials ,(journal-1 'credentials))))
             (prepared ,prepared)))
      :expect (lambda (result)
                (and (pair? result) (eq? (car result) 'error)))))
   '(((proofs ()) (slots (0)))
     ((proofs (((proof ((malformed))) (index 0)))) (slots (#f)))))
  ;; A failed member restores the complete pre-batch retention state.
  (test-submit
    ((*journal* journal-1 'pin-batch!)
     '((-1 *state* hello) (-1 *state* missing)))
    :expect #f)
  (test-submit
    ((*journal* journal-1 'resolve-batch)
     '((-1 *state* hello)) :pinned? #t)
    :expect
    '((results
       (((path (-1 *state* hello)) (content "world") (pinned? #f))))))
  ;; Exact no-proof duplicates skip only after success. A later aligned proof
  ;; for that same path is still verified and rolls back if it throws.
  (test-submit
    (raw journal-1
         '(*call* "pass-1"
                  (lambda (root)
                    (let* ((ledger
                            (sync-eval
                             ((root 'get) '(root object ledger))))
                           (before (sync-digest (ledger)))
                           (failed? #f))
                      (catch #t
                        (lambda ()
                          ((ledger 'pin-batch!)
                           '((0 *state* hello)
                             (0 *state* hello))
                           '(#f ((proof ((malformed))) (index 0)))))
                        (lambda args (set! failed? #t)))
                      (list failed? (equal? before (sync-digest (ledger))))))))
    :expect '(#t #t))
  (test-submit
    (raw journal-1
         '(*call* "pass-1"
                  (lambda (root)
                    (let* ((ledger
                            (sync-eval
                             ((root 'get) '(root object ledger)))))
                      ((ledger 'pin-batch!)
                       '((0 *state* hello) (0 *state* batch alpha)))
                      (let ((before (sync-digest (ledger)))
                            (failed? #f))
                        (catch #t
                          (lambda ()
                            ((ledger 'unpin-batch!)
                             '((0 *state* hello) malformed)))
                          (lambda args (set! failed? #t)))
                        (list failed?
                              (equal? before (sync-digest (ledger)))))))))
    :expect '(#t #t))
  (test-submit
    ((*journal* journal-1 'pin-batch!)
     '((-1 *state* hello) (-1 *state* batch alpha)))
    :expect #t)
  (test-submit
    ((*journal* journal-1 'resolve-batch)
     '((-1 *state* hello) (-1 *state* batch alpha)) :pinned? #t)
    :expect
    '((results
       (((path (-1 *state* hello)) (content "world") (pinned? #t))
        ((path (-1 *state* batch alpha)) (content "a") (pinned? #t))))))
  (test-submit
    ((*journal* journal-1 'unpin-batch!)
     '((-1 *state* hello) (-1 *state* batch alpha)))
    :expect #t)
  (test-submit
    ((*journal* journal-1 'resolve-batch)
     '((-1 *state* hello) (-1 *state* batch alpha)) :pinned? #t)
    :expect
    '((results
       (((path (-1 *state* hello)) (content "world") (pinned? #f))
        ((path (-1 *state* batch alpha)) (content "a") (pinned? #f))))))
  ;; Duplicate and reversed proof cuts are idempotent and permutation-safe.
  (let ((unpinned-digest
         (collect
          (raw journal-1
               '(*call* "pass-1"
                        (lambda (root)
                          (let ((ledger
                                 (sync-eval
                                  ((root 'get) '(root object ledger)))))
                            (sync-digest ((ledger '~field!) 'perm)))))))))
    (test-submit
      ((*journal* journal-1 'pin-batch!)
       '((-1 *state* batch alpha) (-1 *state* hello)))
      :expect #t)
    (test-submit
      ((*journal* journal-1 'unpin-batch!)
       '((-1 *state* hello) (-1 *state* batch alpha)
         (-1 *state* hello)))
      :expect #t)
    (test-submit
      (raw journal-1
           '(*call* "pass-1"
                    (lambda (root)
                      (let ((ledger
                             (sync-eval
                              ((root 'get) '(root object ledger)))))
                        (sync-digest ((ledger '~field!) 'perm))))))
      :expect unpinned-digest))
  (test-submit
    ((*journal* journal-1 'pin-batch!)
     '((-1 *state* batch) (-1 *state* batch alpha)))
    :expect #t)
  (test-submit
    ((*journal* journal-1 'unpin-batch!)
     '((-1 *state* batch) (-1 *state* batch alpha)))
    :expect #t)
  (test-submit
    ((*journal* journal-1 'resolve-batch)
     '((-1 *state* batch alpha)) :pinned? #t)
    :expect
    '((results
       (((path (-1 *state* batch alpha)) (content "a") (pinned? #f))))))
  (test-submit
    ((*journal* journal-1 'resolve-batch)
     '((-1 *state* hello)) :proof? #f)
    :expect (lambda (result)
              (and (list? result) (eq? (car result) 'error))))
  ;; Missing content returns a digest-preserving non-membership proof.
  (test-submit
    ((*journal* journal-1 'resolve)
     '(-1 *state* missing) :pinned? #f :proof? #t)
    :expect (lambda (result)
              (and (equal? (cadr (assoc 'content result)) '(nothing))
                   (list? (cadr (assoc 'proof result))))))
  (test-submit ((*journal* journal-1 'set!) '(*state* do pin this) "yes") :expect #t)
  (test-submit ((*journal* journal-1 'set!) '(*state* do pin that) "yes") :expect #t)
  (test-submit ((*journal* journal-1 'set!) '(*state* do not pin) "no") :expect #t)

  (test-submit ((*journal* journal-1 'step!)) :expect 2)

  ;; A missing bridge is an ordinary routed-action failure, not malformed authentication.
  (test-submit
    ((alice journal-1 journal-3 'get) '(*state* hello))
    :expect (lambda (result) (and (list? result) (eq? (car result) 'error))))
  (test-submit ((*journal* journal-2 'set!) '(*state* a b c) 42) :expect #t)

  (test-submit ((*journal* journal-2 'step!)) :expect 1)

  (test-submit ((*journal* journal-1 'bridge!) journal-2) :expect #t)
  (test-submit ((*journal* journal-1 'bridge!) journal-2) :expect #t)
  ;; Only an internally signed continuation can enter the second phase.
  (test-submit
    (raw journal-1
         `((function bridge!)
           (arguments
            ((name journal-2)
             (interface "http://journal-2.test/interface")
             (remote-name journal-1)))
           (authentication
            ((identity (*state* admin))
             (credentials ,(journal-1 'credentials))))
           (prepared
            ((domain sync-web/federation-continuation/v1)
             (operation bridge!)
             (arguments ())
             (signature #u(1 2 3))))))
    :expect (lambda (result)
              (and (pair? result) (eq? (car result) 'error))))
  (test-submit ((*journal* journal-1 'bridge!) journal-2) :expect #t)

  ;; Bridge discovery is public protocol metadata inherited by authenticated
  ;; users, while application tails and mutation remain independently denied.
  (for-each
   (lambda (principal)
     (test-submit ((principal journal-1 'get) '(*bridge*))
       :expect '(directory ((journal-2 directory)) #t)))
   (list alice bob *journal*))
  (test-submit ((alice journal-1 'get) '(*bridge* journal-2 *state*))
    :expect (lambda (result) (and (pair? result) (eq? (car result) 'error))))
  (test-submit ((alice journal-1 'set!) '(*bridge*) '(forbidden))
    :expect (lambda (result) (and (pair? result) (eq? (car result) 'error))))

  (test-submit (share journal-2 '(*state* a) '(*public*) '() #f #t) :expect #t)

  (test-submit ((*journal* journal-1 'step!)) :expect 3)

  ;; Prepared pin proofs are inert until their exact local history index and
  ;; digest are authenticated. In particular, custom and nonterminating object
  ;; code must never run while rejecting unanchored evidence.
  (test-submit
   (raw journal-1
        '(*call* "pass-1"
          (lambda (root)
            (let* ((ledger (sync-eval ((root 'get) '(root object ledger))))
                   (standard (sync-eval ((root 'get) '(root object standard))))
                   (perm ((ledger '~field!) 'perm))
                   (current (- ((ledger 'size)) 1))
                   (path '(1 *state* do pin this))
                   (internal-path ((ledger '~path-normalize) path #f #f))
                   (valid-node ((standard 'deep-slice!) perm internal-path))
                   (valid-proof ((standard 'serialize) valid-node))
                   (root-stub-proof ((standard 'serialize) (sync-cut perm)))
                   (custom-proof
                    ((standard 'serialize)
                     (sync-cons
                      (expression->byte-vector
                       '(lambda (state . args)
                          (error 'pin-proof-executed "Custom proof ran")))
                      (sync-null))))
                   (nonterminating-proof
                    ((standard 'serialize)
                     (sync-cons
                      (expression->byte-vector
                       '(lambda (state . args) (let loop () (loop))))
                      (sync-null)))))
              (define (tag thunk)
                (catch #t
                  (lambda () (thunk) 'unexpected-success)
                  (lambda args (car args))))
              (list
               (tag (lambda ()
                      ((ledger 'pin!) path
                       `((proof ,custom-proof) (index ,current)))))
               (tag (lambda ()
                      ((ledger 'pin!) path
                       `((proof ,nonterminating-proof) (index ,current)))))
               (tag (lambda ()
                      ((ledger 'pin!) path
                       `((proof ,root-stub-proof) (index ,current)))))
               (tag (lambda ()
                      ((ledger 'pin!) path
                       `((proof ,valid-proof) (index malformed)))))
               (tag (lambda ()
                      ((ledger 'pin!) path
                       `((proof ,valid-proof) (index 999999)))))
               (tag (lambda ()
                      ((ledger 'pin!) path
                       `((proof ,valid-proof) (index 1)))))
               (equal?
                ((standard 'deep-get)
                 ((ledger 'read) current valid-node '())
                 `(,current (*crypto* signature)))
                '(unknown))
               ((ledger 'pin!) path
                `((proof ,valid-proof) (index ,current))))))))
   :expect '(integrity-error integrity-error integrity-error integrity-error
             index-error integrity-error #t #t))

  ;; Pin two siblings only after their historical head is no longer latest.
  (test-submit ((*journal* journal-1 'pin!) '(1 *state* do pin this)) :expect #t)
  (test-submit ((*journal* journal-1 'pin!) '(1 *state* do pin that)) :expect #t)
  (test-submit (share journal-1 '(*state* hello) '(journal-2 *state* alice) '() #t #t) :expect #t)
  ;; The first hop exists, but the second does not yet.
  (test-submit
    ((alice journal-1 journal-2 journal-3 'get) '(*state* hello))
    :expect (lambda (result) (and (list? result) (eq? (car result) 'error))))

  (test-submit ((*journal* journal-1 'resolve) '(-1 *transition* previous operation)
     :expression? #f) :expect (lambda (result)
      (and (equal? (cadr (assoc 'function result)) 'synchronize!)
           (equal? (cadr (assoc 'path result)) '(*bridge* journal-2))
           (integer? (cadr (assoc 'accepted-index result))))))
  ;; Explicit `*bridge*` traversal remains accepted as an internal compatibility form.
  (test-submit ((*journal* journal-1 'resolve)
     '(-1 *bridge* journal-2 *state* a b c) :pinned? #f :proof? #f) :expect 42)
  (test-submit ((*journal* journal-1 'resolve) '(-1 *bridge*) :pinned? #f :proof? #f) :expect '(directory ((journal-2 directory)) #t))
  (test-submit ((*journal* journal-1 'resolve)
     '(-1 *bridge* journal-2 *state* a b) :pinned? #f :proof? #f) :expect '(directory ((c value)) #t))

  (test-submit ((*journal* journal-3 'set!) '(*state* d e f) 64) :expect #t)
  (test-submit ((*journal* journal-4 'set!) '(*state* g h i) "hello") :expect #t)
  (test-submit ((*journal* journal-5 'set!) '(*state* g h i) "world") :expect #t)

  (test-submit ((*journal* journal-3 'step!)) :expect 1)
  (test-submit (share journal-3 '(*state* d) '(*public*) '() #f #t) :expect #t)
  (test-submit ((*journal* journal-4 'step!)) :expect 1)
  (test-submit (share journal-4 '(*state* g) '(*public*) '() #f #t) :expect #t)
  (test-submit ((*journal* journal-5 'step!)) :expect 1)
  (test-submit (share journal-5 '(*state* g) '(*public*) '() #f #t) :expect #t)

  (test-submit ((*journal* journal-2 'bridge!) journal-3) :expect #t)
  (test-submit ((*journal* journal-3 'bridge!) journal-4) :expect #t)
  (test-submit ((*journal* journal-3 'bridge!) journal-5) :expect #t)
  (test-submit ((*journal* journal-3 'step!)) :expect 2)
  (test-submit ((*journal* journal-2 'step!)) :expect 2)
  (test-submit ((*journal* journal-1 'step!)) :expect 4)
  (test-submit ((alice journal-1 journal-2 'get) '(*bridge*))
    :expect '(directory ((journal-3 directory) (journal-1 directory)) #t))

  (test-submit ((*journal* journal-2 'resolve)
     '(-1 *bridge* journal-3 *state* d e f) :pinned? #f :proof? #f) :expect 64)

  (test-submit ((*journal* journal-2 'step!)) :expect 3)
  (test-submit ((*journal* journal-1 'step!)) :expect 5)

  (test-submit ((*journal* journal-1 'resolve)
     '(-1 *bridge* journal-2 *bridge* journal-3 *state* d e f)
     :pinned? #f :proof? #f) :expect (lambda (result) (and (list? result) (eq? (car result) 'error))))

  (test-submit (share journal-1 '(*state* hello)
            '(journal-2 journal-3 *state* alice)
            '() #t #t) :expect #t)

  (test-submit ((*journal* journal-2 'step!)) :expect 4)
  (test-submit ((*journal* journal-1 'set!) '(*state* window boundary) "retained") :expect #t)
  (test-submit ((*journal* journal-1 'step!)) :expect 6)
  (test-submit ((*journal* journal-1 'pin!) '(5 *state* window boundary)) :expect #t)

  (test-submit ((*journal* journal-1 'resolve)
     '(-1 journal-2 -1 journal-3 -1
          journal-4 *state* g h i)
     :pinned? #f :proof? #f) :expect (lambda (result) (and (list? result) (eq? (car result) 'error))))
  (test-submit ((*journal* journal-1 'resolve)
     '(-1 journal-2 -1 journal-3 -1
          journal-5 *state* g h i)
     :pinned? #t :proof? #f) :expect (lambda (result) (and (list? result) (eq? (car result) 'error))))

  (test-submit ((*journal* journal-1 'set!) '(*state* tick) 0) :expect #t)
  (test-submit ((*journal* journal-1 'step!)) :expect 7)
  (test-submit ((*journal* journal-1 'set!) '(*state* tick) 1) :expect #t)
  (test-submit ((*journal* journal-1 'step!)) :expect 8)
  (test-submit ((*journal* journal-1 'set!) '(*state* tick) 2) :expect #t)
  (test-submit ((*journal* journal-1 'step!)) :expect 9)
  (test-submit ((*journal* journal-1 'set!) '(*state* tick) 3) :expect #t)

  (test-submit ((*journal* journal-1 'step!)) :expect 10)

  ;; The configured temporary window admits relative retention through -4.
  ;; The explicitly retained -5 object remains known but is outside that window.
  (for-each
    (lambda (entry)
      (test-submit ((*journal* journal-1 'pin!) `(,(car entry) *state* window boundary)) :expect (cadr entry)))
    '((-1 #t) (-2 #t) (-3 #t) (-4 #t) (-5 #f)))

  (let ((path '(6 *bridge* journal-2 *state* a b c)))
    (test-submit ((*journal* journal-1 'pin!) path) :expect #t)
    (test-submit ((*journal* journal-1 'resolve) path :pinned? #t :proof? #f) :expect (lambda (result) (cadr (assoc 'pinned? result)))))

  (test-submit ((*journal* journal-1 'resolve) '(1 *state* do pin)
     :pinned? #f :proof? #f) :expect '(directory ((this value) (that value)) #t))
  (test-submit ((*journal* journal-1 'resolve) '(1 *state* do pin this) :pinned? #f :proof? #f) :expect "yes")
  (test-submit ((*journal* journal-1 'resolve) '(1 *state* do pin that) :pinned? #f :proof? #f) :expect "yes")
  (test-submit ((*journal* journal-1 'resolve) '(1 *state* do not pin) :pinned? #f :proof? #f) :expect '(unknown))
  (test-submit ((*journal* journal-1 'unpin!) '(1 *state* do pin that)) :expect #t)
  (test-submit ((*journal* journal-1 'resolve) '(1 *state* do pin this) :pinned? #f :proof? #f) :expect "yes")
  (test-submit ((*journal* journal-1 'resolve) '(1 *state* do pin that) :pinned? #f :proof? #f) :expect '(unknown))

  ;; The rotatable Interface credential is private durable Root state, separate
  ;; from Root authentication, and remains synchronized with its verifier.
  (test-submit
    (raw journal-1
         '(*call* "pass-1"
                  (lambda (root) ((root 'get) '(interface credential)))))
    :expect "http://journal-1.test/interface")
  (test-submit ((*journal* journal-1 '*secret*) '((secret "pass-1")))
    :expect (lambda (result) (and (pair? result) (eq? (car result) 'error))))
  (test-submit ((*journal* journal-1 '*secret*) '((secret "pass-1-new"))) :expect #t)
  (test-report)
  (test-submit
    (raw journal-1
         '((function get)
           (arguments ((path (*state* hello)) (expression? #t)))
           (authentication
            ((credentials "http://journal-1.test/interface")))))
    :expect (lambda (result)
              (and (list? result) (eq? (car result) 'error))))
  (journal-1 'credentials "pass-1-new")
  (test-submit ((*journal* journal-1 'get) '(*state* hello)) :expect "world")
  (test-submit
    (raw journal-1
         '(*call* "pass-1"
                  (lambda (root) ((root 'get) '(interface credential)))))
    :expect "pass-1-new")
  (test-report)
  (test-submit ((*journal* journal-1 '*window-set*) '((value 2))) :expect #t)
  (test-submit ((*journal* journal-1 '*secret*)
                '((secret "http://journal-1.test/interface"))) :expect #t)
  (test-report)
  (journal-1 'credentials "http://journal-1.test/interface")

  ;; These two raw calls intentionally verify unauthenticated rejection.
  (test-submit ((*anonymous* journal-1 'size)) :expect 10)
  (test-submit
    ((*anonymous* journal-1 'config) :path '(public window))
    :expect (lambda (result) (and (list? result) (eq? (car result) 'error))))
  (test-submit ((*journal* journal-1 'config) :path '(public window)) :expect 2)
  (test-submit
    ((*anonymous* journal-1 'config) :path '(private secret-key))
    :expect (lambda (result) (and (list? result) (eq? (car result) 'error))))
  (test-submit ((*journal* journal-1 'config) :path '(private secret-key)) :expect '())

  (test-submit ((*journal* journal-1 'get) '(*state* hello)) :expect "world")
  (test-submit ((*journal* journal-1 'resolve) '(-1 *state* hello) :pinned? #f :proof? #f) :expect "world")
  (test-submit ((*journal* journal-1 'pin!) '(-1 *state* hello)) :expect #t)
  (test-submit ((*journal* journal-1 'resolve) '(-1 *state* hello)
     :pinned? #t :proof? #f) :expect '((content "world") (pinned? #t)))
  (test-submit ((*journal* journal-1 'unpin!) '(-1 *state* hello)) :expect #t)

  (test-submit ((alice journal-1 'set!) '(*state* alice data) "public data") :expect #t)
  (test-submit ((alice journal-1 'set!) '(*state* alice *private* data) "private data") :expect #t)
  (test-submit ((bob journal-1 'set!) '(*state* alice data) "bob's data") :expect (lambda (result) (and (list? result) (eq? (car result) 'error))))
  (test-submit ((alice journal-1 'get) '(*state* alice data)) :expect "public data")
  (test-submit ((alice journal-1 'get) '(*state* alice *private* data)) :expect "private data")
  (test-submit ((bob journal-1 'get) '(*state* alice *private* data)) :expect (lambda (result) (and (list? result) (eq? (car result) 'error))))

  (test-submit ((alice journal-1 'set!) '(*state* alice foo *private*) "my private data") :expect #t)

  ;; Cross-user reads require an explicit authorization rule. A conditional
  ;; write also requires read authority because equality reveals current state.
  (test-submit ((*journal* journal-1 'set!) '(*state* alice conditional-auth) "initial")
               :expect #t)
  (test-submit
    ((alice journal-1 'authorize!)
     '((user (*state* alice))
       (rule ((principal (*state* bob)) (path (conditional-auth))
              (get #f) (set! #t) (resolve #f)))))
    :expect #t)
  (test-submit ((bob journal-1 'set!) '(*state* alice conditional-auth) "plain")
               :expect #t)
  (test-submit
    ((bob journal-1 'set!) '(*state* alice conditional-auth) "conditional"
     :expected "plain")
    :expect (lambda (result)
              (and (list? result) (eq? (car result) 'error))))
  (test-submit
    ((bob journal-1 'set-batch!)
     '((paths ((*state* alice conditional-auth)))
       (values (batch-plain)) (expression? #t)))
    :expect #t)
  (test-submit
    ((bob journal-1 'set-batch!)
     '((paths ((*state* alice conditional-auth)))
       (values (batch-conditional)) (expected (batch-plain))
       (expression? #t)))
    :expect (lambda (result)
              (and (list? result) (eq? (car result) 'error))))
  (test-submit
    ((alice journal-1 'deauthorize!)
     '((user (*state* alice))
       (rule ((principal (*state* bob)) (path (conditional-auth))
              (get #f) (set! #t) (resolve #f)))))
    :expect #t)
  (test-submit ((bob journal-1 'get) '(*state* alice data)) :expect (lambda (result) (and (list? result) (eq? (car result) 'error))))
  (test-submit (share journal-1 '(*state* alice) '(*state* bob) '(data) #t #f) :expect #t)
  (test-submit ((bob journal-1 'get) '(*state* alice data)) :expect "public data")
  (test-submit
    ((bob journal-1 'set!) '(*state* alice data) "denied"
     :expected "public data")
    :expect (lambda (result)
              (and (list? result) (eq? (car result) 'error))))

  ;; Descendant grants admit ordinary ancestor listings without explicit parent
  ;; grants; opening each listed child still requires its own authorization.
  (for-each
    (lambda (entry)
      (test-submit ((*journal* journal-1 'set!) `(*state* projection data ,(car entry) key-0) (cadr entry)) :expect #t))
    '((public "public") (journal-2 "private") (hidden "hidden")))
  (test-submit (share journal-1 '(*state* projection) '(*public*) '(data public) #t #t) :expect #t)
  (test-submit (share journal-1 '(*state* projection) '(*state* bob) '(data journal-2) #t #t) :expect #t)
  (test-submit ((bob journal-1 'get) '(*state* projection data)) :expect (lambda (result)
      (and (eq? (car result) 'directory)
           (= (length (cadr result)) 3)
           (assoc 'public (cadr result))
           (assoc 'journal-2 (cadr result))
           (assoc 'hidden (cadr result)))))
  (test-submit ((carol journal-1 'get) '(*state* projection data)) :expect (lambda (result)
      (and (eq? (car result) 'directory)
           (= (length (cadr result)) 3)
           (assoc 'public (cadr result))
           (assoc 'journal-2 (cadr result))
           (assoc 'hidden (cadr result)))))
  (test-submit ((bob journal-1 'get) '(*state* projection data hidden key-0)) :expect (lambda (result) (and (list? result) (eq? (car result) 'error))))

  (test-submit ((alice journal-1 'authorizations) '((user (*state* alice)))) :expect '(((principal (*state* bob)) (path (data))
       (get #t) (set! #f) (resolve #f))))
  (test-submit ((alice journal-1 'deauthorize!)
     '((user (*state* alice))
       (rule ((principal (*state* bob)) (path (data))
              (get #t) (set! #f) (resolve #f))))) :expect #t)
  (test-submit ((bob journal-1 'get) '(*state* alice data)) :expect (lambda (result) (and (list? result) (eq? (car result) 'error))))

  ;; The ownerless state directory remains navigable when the principal has
  ;; descendant authority, while reserved entries stay hidden.
  (test-submit ((bob journal-1 'get) '(*state*)) :expect (lambda (result)
      (and (eq? (car result) 'directory)
           (assoc 'alice (cadr result))
           (assoc 'projection (cadr result))
           (not (assoc '*directory* (cadr result))))))
  (test-submit ((bob journal-1 'resolve) '(-1 *state*) :pinned? #t :proof? #t) :expect (lambda (result)
      (let ((content (and (pair? result) (assoc 'content result)
                          (cadr (assoc 'content result)))))
        (and content (eq? (car content) 'directory)
             (not (assoc '*directory* (cadr content)))))))
  (test-submit
    ((bob journal-1 'resolve-batch)
     '((-1 *state*) (-1 *state*)))
    :expect
    (lambda (result)
      (let* ((results (and (list? result) (assoc 'results result)
                           (cadr (assoc 'results result))))
             (directory (and (pair? results)
                             (cadr (assoc 'content (car results))))))
        (and (pair? directory) (eq? (car directory) 'directory)
             (not (assoc '*directory* (cadr directory)))))))

  ;; A mixed-owner batch and non-admin bridge operation fail atomically.
  (test-submit
    ((bob journal-1 'get-batch)
     '((*state* bob stuff) (*state* alice stuff)))
    :expect (lambda (result)
              (and (list? result) (eq? (car result) 'error))))
  (test-submit ((bob journal-1 'set-batch!)
     '((paths ((*state* bob stuff) (*state* alice stuff)))
       (values ("val1" "val2")) (expression? #t))) :expect (lambda (result) (and (list? result) (eq? (car result) 'error))))
  (test-submit ((alice journal-1 'bridge!) journal-2) :expect (lambda (result) (and (list? result) (eq? (car result) 'error))))

  (test-submit ((*journal* journal-1 '*admins-get*)) :expect '())
  (test-submit ((*journal* journal-1 '*admins-set*) '((admins ((*state* alice))))) :expect #t)
  (test-submit ((alice journal-1 '*admins-get*)) :expect '((*state* alice)))

  ;; A refused reinstall leaves runtime-managed administrators unchanged.
  (test-submit (update-interface journal-1 '(bob))
    :expect (lambda (result)
              (and (list? result) (eq? (car result) 'error))))
  (test-submit ((alice journal-1 '*admins-get*)) :expect '((*state* alice)))
  (test-submit ((alice journal-1 '*window-set*) '((value 3))) :expect #t)
  (test-submit ((alice journal-1 'config) :path '(public window)) :expect 3)
  (test-submit ((alice journal-1 '*window-set*) '((value 0))) :expect (lambda (result) (and (list? result) (eq? (car result) 'error))))

  ;; User payloads are Tree-native bytes; expression? is only an Interface codec.
  (test-submit ((*journal* journal-1 'set!) '(*state* bytes raw)
     #u(0 1 2 255) :expression? #f) :expect #t)
  (test-submit ((*journal* journal-1 'get) '(*state* bytes raw)
     :expression? #f) :expect #u(0 1 2 255))
  (test-submit ((*journal* journal-1 'set!) '(*state* bytes false-value) #f)
    :expect #t)
  (test-submit ((*journal* journal-1 'get) '(*state* bytes false-value))
    :expect (lambda (result) (eq? result #f)))
  (test-submit ((*journal* journal-1 'step!)) :expect 11)
  (test-submit
    ((*journal* journal-1 'resolve)
     '(-1 *state* bytes raw) :expression? #f :pinned? #f :proof? #t)
    :expect
    (lambda (result)
      (and (equal? (cadr (assoc 'content result)) #u(0 1 2 255))
           (list? (cadr (assoc 'proof result))))))

  ;; Removed metadata arguments fail explicitly instead of being ignored.
  (test-submit ((*journal* journal-1 'get) '(*state* bytes raw) :meta? #t)
    :expect (lambda (result) (and (list? result) (eq? (car result) 'error))))
  (test-submit ((*journal* journal-1 'set!) '(*state* bytes raw)
     :meta '((format ((mime "text/plain")))))
    :expect (lambda (result) (and (list? result) (eq? (car result) 'error))))
  (test-submit ((*journal* journal-1 'resolve)
     '(-1 *state* bytes raw) :meta? #t)
    :expect (lambda (result) (and (list? result) (eq? (car result) 'error))))
  (test-submit ((*journal* journal-1 'set-batch!)
     '((paths ((*state* bytes legacy)))
       (values (#u(1)))
       (metas (((format ((mime "application/octet-stream"))))))))
    :expect (lambda (result) (and (list? result) (eq? (car result) 'error))))

  ;; Concise end-to-end user federation. Alice originates at journal-6 and
  ;; reaches Bob's data at journal-7 through the explicit numeric route.
  (test-submit ((*journal* journal-6 'set!) '(*state* alice seed) "origin") :expect #t)
  (test-submit ((*journal* journal-7 'set!) '(*state* bob value) 199) :expect #t)

  (test-submit ((*journal* journal-6 'step!)) :expect 1)
  (test-submit ((*journal* journal-7 'step!)) :expect 1)

  (test-submit ((*journal* journal-6 'bridge!) journal-7) :expect #t)
  (test-submit ((*journal* journal-7 'authorize!)
     '((user (*state* bob))
       (rule ((principal (journal-6 *state* alice))
              (key-index (-10 -1)) (path ())
              (get #t) (set! #t) (resolve #t))))) :expect #t)
  (test-submit ((*journal* journal-7 'authorize!)
     '((user (*state* bob))
       (rule ((principal (journal-6))
              (key-index (-10 -1)) (path ())
              (get #f) (set! #f) (resolve #t))))) :expect #t)
  (test-submit ((*journal* journal-7 'step!)) :expect 2)
  (test-submit ((*journal* journal-6 'bridge!) journal-7) :expect #t)
  (test-submit ((*journal* journal-6 'step!)) :expect 2)
  ;; Submission order governs observation even when the later local read
  ;; completes before the earlier federated read.
  (test-submit ((alice journal-6 journal-7 'get) '(*state* bob value)) :schedule '(2 0) :expect 199)
  (test-submit ((*journal* journal-7 'get) '(*state* bob value)) :expect 199)
  (test-submit
    ((alice journal-6 'resolve-batch)
     '((-1 journal-7 -1 *state* bob value)
       (-1 journal-7 -1 *state* bob missing)
       (-1 journal-7 -1 *state* bob value)))
    :expect
    '((results
       (((path (-1 journal-7 -1 *state* bob value)) (content 199))
        ((path (-1 journal-7 -1 *state* bob missing)) (content (nothing)))
        ((path (-1 journal-7 -1 *state* bob value)) (content 199))))))
  (test-submit
    ((alice journal-6 'resolve-batch)
     '((-1 journal-7 -1 *state* bob value)
       (-1 *state* bob denied)))
    :expect (lambda (result)
              (and (list? result) (eq? (car result) 'error))))
  (test-submit
    ((alice journal-6 'resolve-batch)
     '((-1 *state* alice seed)
       (-1 journal-7 -1 *state* bob value)
       (-1 journal-7 1 *state* bob value)))
    :expect
    '((results
       (((path (-1 *state* alice seed)) (content "origin"))
        ((path (-1 journal-7 -1 *state* bob value)) (content 199))
        ((path (-1 journal-7 1 *state* bob value)) (content 199))))))
  (test-submit
    ((*journal* journal-6 'pin-batch!)
     '((-1 *state* alice seed)
       (-1 journal-7 -1 *state* bob value)))
    :schedule '(0 #f)
    :expect (lambda (result)
              (and (list? result) (eq? (car result) 'error))))
  (test-submit
    ((*journal* journal-6 'resolve-batch)
     '((-1 *state* alice seed)) :pinned? #t)
    :expect
    '((results
       (((path (-1 *state* alice seed))
         (content "origin") (pinned? #f))))))
  (test-submit
    ((*journal* journal-6 'pin-batch!)
     '((-1 *state* alice seed)
       (-1 journal-7 -1 *state* bob value)))
    :expect #t)
  (test-submit
    ((alice journal-6 'resolve-batch)
     '((-1 *state* alice seed)
       (-1 journal-7 -1 *state* bob value)) :pinned? #t)
    :expect
    '((results
       (((path (-1 *state* alice seed)) (content "origin") (pinned? #t))
        ((path (-1 journal-7 -1 *state* bob value))
         (content 199) (pinned? #t))))))
  (test-submit
    ((*journal* journal-6 'unpin-batch!)
     '((-1 *state* alice seed)
       (-1 journal-7 -1 *state* bob value)))
    :expect #t)
  ;; Local, latest-remote, and historical-remote members form three ordered
  ;; slots over two distinct prepared proof groups and one ordinary local path.
  (let ((mixed
         '((-1 *state* alice seed)
           (-1 journal-7 -1 *state* bob value)
           (-1 journal-7 1 *state* bob value)
           (-1 journal-7 -1 *state* bob value))))
    (test-submit ((*journal* journal-6 'pin-batch!) mixed) :expect #t)
    (test-submit
     ((alice journal-6 'resolve-batch) mixed :pinned? #t)
     :expect (lambda (result)
               (let ((items (cadr (assoc 'results result))))
                 (and (= (length items) 4)
                      (equal? (map (lambda (item)
                                     (cadr (assoc 'pinned? item)))
                                   items)
                              '(#t #t #t #t))))))
    (test-submit ((*journal* journal-6 'unpin-batch!) mixed) :expect #t))
  ;; One remote route-group proof is prepared once and referenced by all 1,024
  ;; ordered duplicate slots across the blocking authenticated self-call.
  (for-each
   (lambda (count)
     (let ((paths
            (make-list count '(-1 journal-7 -1 *state* bob value))))
       (test-submit ((*journal* journal-6 'pin-batch!) paths) :expect #t)
       (test-submit ((*journal* journal-6 'unpin-batch!) paths) :expect #t)))
   '(1 128 1024))

  ;; The shared public cap accepts exactly 1,024 paths and rejects 1,025 for
  ;; every dedicated endpoint. Duplicate reads/proofs/retention cuts preserve
  ;; their endpoint-specific order and idempotence semantics.
  (let ((paths-1024 (make-list 1024 '(*state* hello)))
        (committed-1024 (make-list 1024 '(-1 *state* hello)))
        (too-many-stage (make-list 1025 '(*state* hello)))
        (too-many-committed (make-list 1025 '(-1 *state* hello)))
        (error-result?
         (lambda (result)
           (and (list? result) (eq? (car result) 'error)))))
    (test-submit
      ((*journal* journal-1 'get-batch) paths-1024)
      :expect
      (lambda (result)
        (let ((values (and (list? result) (assoc 'results result))))
          (and values (= (length (cadr values)) 1024)))))
    (test-submit
      ((*journal* journal-1 'resolve-batch) committed-1024)
      :expect
      (lambda (result)
        (let ((values (and (list? result) (assoc 'results result))))
          (and values (= (length (cadr values)) 1024)))))
    (test-submit
      ((*journal* journal-1 'trace-batch)
       `((index -1) (paths ,committed-1024)))
      :expect (lambda (result) (and (list? result) (pair? result))))
    (test-submit ((*journal* journal-1 'pin-batch!) committed-1024)
                 :expect #t)
    (test-submit ((*journal* journal-1 'unpin-batch!) committed-1024)
                 :expect #t)
    (for-each
     (lambda (action) (test-submit action :expect error-result?))
     (list
      ((*journal* journal-1 'get-batch) too-many-stage)
      ((*journal* journal-1 'set-batch!)
       `((paths ,too-many-stage) (values ,too-many-stage)
         (expression? #t)))
      ((*journal* journal-1 'resolve-batch) too-many-committed)
      ((*journal* journal-1 'trace-batch)
       `((index -1) (paths ,too-many-committed)))
      ((*journal* journal-1 'pin-batch!) too-many-committed)
      ((*journal* journal-1 'unpin-batch!) too-many-committed)))
    (test-submit ((*journal* journal-1 'resolve-batch) '())
                 :expect '((results ())))
    (test-submit ((*journal* journal-1 'pin-batch!) '()) :expect #t)
    (test-submit ((*journal* journal-1 'unpin-batch!) '()) :expect #t)
    (test-submit
      ((*journal* journal-1 'trace-batch) '((index -1) (paths ())))
      :expect error-result?))

  ;; A dropped return message becomes an immediate transport error without a
  ;; synthetic timeout duration.
  (test-submit
    ((alice journal-6 journal-7 'get) '(*state* bob value))
    :schedule '(0 #f)
    :expect (lambda (result)
              (and (list? result) (eq? (car result) 'error))))

  ;; Federated mutation uses the same action shape; terminal policy decides it.
  (test-submit ((alice journal-6 journal-7 'set!) '(*state* bob value) 200) :expect #t)
  (test-submit ((alice journal-6 journal-7 'get) '(*state* bob value)) :expect 200)
  (test-submit
    ((carol journal-6 journal-7 'set!) '(*state* bob value) 201)
    :expect (lambda (result) (and (list? result) (eq? (car result) 'error))))

  ;; A legacy-shaped Ledger is rejected explicitly; conversion/reset policy is
  ;; a deployment decision rather than an implicit reinterpretation.
  (test-submit
    (raw journal-4
         '(*call* "pass-4"
                  (lambda (root)
                    (let* ((ledger
                            (sync-eval
                             ((root 'get) '(root object ledger))))
                           (node (ledger))
                           (legacy
                            (sync-cons
                             (sync-car node)
                             (sync-cons
                              (sync-car (sync-cdr node))
                              (sync-cons (sync-null)
                                         (ledger '(1 1)))))))
                      ((root 'set!) '(root object ledger) legacy)
                      (let* ((stored
                              (sync-eval
                               ((root 'get) '(root object ledger))))
                             (candidate (stored '(1 1))))
                        (list (sync-pair? candidate)
                              (byte-vector? (sync-car candidate))
                              (sync-null? (sync-car candidate))))))))
    :expect '(#t #f #t))
  (test-report)
  (let ((fingerprint
         (collect
          (raw journal-4
               '(*call* "pass-4"
                        (lambda (root) (sync-digest (root))))))))
    (test-submit (update-interface journal-4 '() 4)
      :expect (lambda (result)
                (and (list? result) (eq? (car result) 'error))))
    (test-report)
    (test-submit
      (raw journal-4
           '(*call* "pass-4"
                    (lambda (root) (sync-digest (root)))))
      :expect fingerprint))

  (test-report)))
