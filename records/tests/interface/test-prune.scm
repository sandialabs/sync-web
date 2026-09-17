(lambda (make-interface-harness)

  (with-let (make-interface-harness :journals 2 :users '(alice bob)
                                    :admins '((*state* bob)) :window #f)

    (define (error-result? result)
      (and (list? result) (eq? (car result) 'error)))

    (define (collect action)
      (test-report)
      (test-submit action)
      (test-await))

    (define (build-history journal)
      (test-submit ((*journal* journal 'put!) '(*state* alice leaf) "leaf-0")
                   :expect #t)
      (test-submit ((*journal* journal 'put!) '(*state* alice keep) "keep-0")
                   :expect #t)
      (test-submit ((*journal* journal 'put!) '(*state* alice dir a) "a-0")
                   :expect #t)
      (test-submit ((*journal* journal 'put!) '(*state* alice dir b) "b-0")
                   :expect #t)
      (test-submit ((*journal* journal 'step!)) :expect 1)
      (test-submit ((*journal* journal 'put!) '(*state* alice leaf) "leaf-1")
                   :expect #t)
      (test-submit ((*journal* journal 'put!) '(*state* alice keep) "keep-1")
                   :expect #t)
      (test-submit ((*journal* journal 'put!) '(*state* alice dir a) "a-1")
                   :expect #t)
      (test-submit ((*journal* journal 'put!) '(*state* alice dir b) "b-1")
                   :expect #t)
      (test-submit ((*journal* journal 'step!)) :expect 2)
      ;; Retain complete namespace evidence in permanent history at both indexes.
      (test-submit ((*journal* journal 'pin!) '(0 *state* alice)) :expect #t)
      (test-submit ((*journal* journal 'pin!) '(1 *state* alice)) :expect #t))

    (define (retention-observation journal paths)
      (raw journal
           `(*call* ,(journal 'root-secret)
                     (lambda (root)
                       (let* ((ledger
                               (sync-eval
                                ((root 'get) '(root object ledger))))
                              (standard
                               (sync-eval ((ledger '~field!) 'standard))))
                         (map
                          (lambda (field)
                            (map
                             (lambda (path)
                               (let ((value
                                      ((standard 'deep-get)
                                       ((ledger '~field!) field)
                                       ((ledger '~path-normalize) path #f #f))))
                                 (if (byte-vector? value)
                                     (byte-vector->expression value)
                                     value)))
                             ',paths))
                          '(perm temp)))))))

    (define (retention-digests journal)
      (raw journal
           `(*call* ,(journal 'root-secret)
                     (lambda (root)
                       (let ((ledger
                              (sync-eval
                               ((root 'get) '(root object ledger)))))
                         (map
                          (lambda (field)
                            (sync-digest ((ledger '~field!) field)))
                          '(perm temp stage)))))))

    (define (retention-protected? journal)
      (raw journal
           `(*call* ,(journal 'root-secret)
                     (lambda (root)
                       (let* ((ledger
                               (sync-eval
                                ((root 'get) '(root object ledger))))
                              (standard
                               (sync-eval ((ledger '~field!) 'standard))))
                         (let loop ((fields '(perm temp)))
                           (or (null? fields)
                               (and
                                (byte-vector?
                                 ((standard 'deep-get)
                                  ((ledger '~field!) (car fields))
                                  '(1 (*crypto* signature))))
                                (equal?
                                 (byte-vector->expression
                                  ((standard 'deep-get)
                                   ((ledger '~field!) (car fields))
                                   '(1 (*state* alice leaf))))
                                 "leaf-1")
                                (loop (cdr fields))))))))))

    (for-each build-history (list journal-1 journal-2))
    (test-report)

    ;; Pruning has no ordinary Authorization field. A broad current grant and
    ;; namespace ownership still do not confer it.
    (test-submit
     ((alice journal-1 'authorize!)
      '((user (*state* alice))
        (rule ((principal (*state* alice)) (path ()) (prune! #t)))))
     :expect error-result?)
    (test-submit
     ((alice journal-1 'authorize!)
      '((user (*state* alice))
        (rule ((principal (*state* alice)) (path ())
               (put! #t) (use! ((read-only? #f))) (retrieve #t) (run! #t)))))
     :expect #t)
    (test-submit ((alice journal-1 'prune!) '(-1 *state* alice leaf))
                 :expect error-result?)
    (test-submit
     ((alice journal-1 'prune-batch!) '((-1 *state* alice leaf)))
     :expect error-result?)
    (test-submit ((*anonymous* journal-1 'prune!) '(-1 *state* alice leaf))
                 :expect error-result?)
    (test-submit
     ((bob journal-2 journal-1 'prune!) '(-1 *state* alice leaf))
     :expect error-result?)

    ;; Malformed, out-of-range, and malformed-batch requests fail before any
    ;; retained evidence changes. Empty batches are valid and idempotent.
    (test-submit ((bob journal-1 'prune!) '()) :expect error-result?)
    (test-submit ((bob journal-1 'prune!) '(2 *state* alice leaf))
                 :expect error-result?)
    (test-submit ((bob journal-1 'prune-batch!) 'not-a-list)
                 :expect error-result?)
    (test-submit
     ((bob journal-1 'prune-batch!)
      '((1 *state* alice leaf) (2 *state* alice keep)))
     :expect error-result?)
    (test-submit ((bob journal-1 'prune-batch!) '()) :expect #t)
    (test-submit
     ((*journal* journal-1 'retrieve)
      '(1 *state* alice leaf) :pinned? #f :proof? #f)
     :expect "leaf-1")
    (test-submit
     (retention-observation
      journal-1 '((1 *state* alice leaf) (1 *state* alice keep)))
     :expect '(("leaf-1" "keep-1") ("leaf-1" "keep-1")))

    ;; Unpin applies the same protection before any permanent-retention cut.
    ;; A protected member anywhere in a batch leaves every Ledger field exact.
    (let ((before (collect (retention-digests journal-1))))
      (test-submit ((bob journal-1 'unpin!) '(-1 *crypto* signature))
                   :expect error-result?)
      (test-submit ((bob journal-1 'unpin!) '(-1))
                   :expect error-result?)
      (test-submit ((bob journal-1 'unpin!) '(-1 *bridge*))
                   :expect error-result?)
      (test-submit ((bob journal-1 'unpin!) '(-1 *bridge* journal-2))
                   :expect error-result?)
      (test-submit ((bob journal-1 'unpin!)
                    '(-1 *bridge* journal-2 -1))
                   :expect error-result?)
      (test-submit ((bob journal-1 'unpin!)
                    '(-1 *bridge* journal-2 -1 *crypto* signature))
                   :expect error-result?)
      (test-submit ((bob journal-1 'unpin!)
                    '(-1 *bridge* journal-2 -1 journal-2))
                   :expect error-result?)
      (test-submit ((bob journal-1 'unpin!)
                    '(-1 *bridge* journal-2 -1 journal-2 -1))
                   :expect error-result?)
      (test-submit ((bob journal-1 'unpin!)
                    '(-1 *bridge* journal-2 -1 journal-2 -1
                      *crypto* signature))
                   :expect error-result?)
      (test-submit
       ((bob journal-1 'unpin-batch!)
        '((-1 *state* alice leaf) (-1 *crypto* signature)))
       :expect error-result?)
      (test-submit
       ((bob journal-1 'unpin-batch!)
        '((-1 *state* alice leaf)
          (-1 *bridge* journal-2 -1 *crypto* signature)))
       :expect error-result?)
      (test-submit
       ((bob journal-1 'unpin-batch!)
        '((-1 *state* alice leaf)
          (-1 *bridge* journal-2 -1 journal-2)))
       :expect error-result?)
      (test-submit
       ((bob journal-1 'unpin-batch!)
        '((-1 *state* alice leaf)
          (-1 *bridge* journal-2 -1 journal-2 -1)))
       :expect error-result?)
      (test-submit (retention-digests journal-1) :expect before)
      (test-submit (retention-protected? journal-1) :expect #t))

    ;; Ordinary state unpin still removes permanent retention only.
    (test-submit ((bob journal-2 'unpin!) '(1 *state* alice keep)) :expect #t)
    (test-submit
     (retention-observation journal-2 '((1 *state* alice keep)))
     :expect '(((unknown)) ("keep-1")))

    ;; Direct and ancestral crypto targets fail before either retained field is
    ;; prepared. The same rule covers local and retained bridge history.
    (let ((before (collect (retention-digests journal-1))))
      (test-submit ((bob journal-1 'prune!) '(-1 *crypto* signature))
                   :expect error-result?)
      (test-submit ((bob journal-1 'prune!) '(-1))
                   :expect error-result?)
      (test-submit ((bob journal-1 'prune!) '(-1 *bridge*))
                   :expect error-result?)
      (test-submit ((bob journal-1 'prune!) '(-1 *bridge* journal-2))
                   :expect error-result?)
      (test-submit ((bob journal-1 'prune!)
                    '(-1 *bridge* journal-2 -1))
                   :expect error-result?)
      (test-submit ((bob journal-1 'prune!)
                    '(-1 *bridge* journal-2 -1 *crypto* signature))
                   :expect error-result?)
      (test-submit ((bob journal-1 'prune!)
                    '(-1 *bridge* journal-2 -1 journal-2))
                   :expect error-result?)
      (test-submit ((bob journal-1 'prune!)
                    '(-1 *bridge* journal-2 -1 journal-2 -1))
                   :expect error-result?)
      (test-submit ((bob journal-1 'prune!)
                    '(-1 *bridge* journal-2 -1 journal-2 -1
                      *crypto* signature))
                   :expect error-result?)
      (test-submit
       ((bob journal-1 'prune-batch!)
        '((-1 *state* alice leaf) (-1 *crypto* signature)))
       :expect error-result?)
      (test-submit
       ((bob journal-1 'prune-batch!)
        '((-1 *state* alice leaf)
          (-1 *bridge* journal-2 -1 journal-2)))
       :expect error-result?)
      (test-submit
       ((bob journal-1 'prune-batch!)
        '((-1 *state* alice leaf)
          (-1 *bridge* journal-2 -1 journal-2 -1)))
       :expect error-result?)
      (test-submit (retention-digests journal-1) :expect before)
      (test-submit (retention-protected? journal-1) :expect #t))

    ;; A failure while preparing the temporary candidate leaves both Ledger
    ;; fields unchanged after the permanent candidate already succeeded.
    (test-submit
     (raw journal-1
          '(*call* "pass-1"
                   (lambda (root)
            (let* ((ledger (sync-eval ((root 'get) '(root object ledger))))
                   (standard (sync-eval ((ledger '~field!) 'standard)))
                   (path ((ledger '~path-normalize)
                          '(1 *state* alice leaf) #f #f))
                   (original-temp ((ledger '~field!) 'temp))
                   (perm-before ((ledger '~field!) 'perm)))
              ((ledger '~field!) 'temp (sync-cut original-temp))
              (let ((temp-before ((ledger '~field!) 'temp))
                    (failed
                     (catch #t
                       (lambda () ((ledger 'prune!) '(1 *state* alice leaf)) #f)
                       (lambda args #t))))
                (let ((unchanged
                       (and failed
                            (equal? (sync-digest perm-before)
                                    (sync-digest ((ledger '~field!) 'perm)))
                            (equal? (sync-digest temp-before)
                                    (sync-digest ((ledger '~field!) 'temp)))
                            (equal?
                             (byte-vector->expression
                              ((standard 'deep-get)
                               ((ledger '~field!) 'perm) path))
                             "leaf-1"))))
                  ((ledger '~field!) 'temp original-temp)
                  ((root 'set!) '(root object ledger) (ledger))
                  unchanged))))))
     :expect #t)

    ;; Stage a successor after the committed heads. Pruning must not alter it.
    (test-submit ((*journal* journal-1 'put!) '(*state* alice leaf) "staged")
                 :expect #t)
    (test-submit ((*journal* journal-1 'put!) '(*state* alice stage-only) "keep")
                 :expect #t)
    (test-report)

    (let ((before
           (collect
            (raw journal-1
                 '(*call* "pass-1"
                          (lambda (root)
                   (let* ((ledger (sync-eval ((root 'get) '(root object ledger))))
                          (standard (sync-eval ((ledger '~field!) 'standard)))
                          (perm ((ledger '~field!) 'perm)))
                     (list
                      (sync-digest (root))
                      (sync-digest (ledger))
                      (sync-digest ((ledger '~field!) 'stage))
                      (sync-digest perm)
                      (sync-digest ((ledger '~field!) 'temp))
                      ((ledger 'size))
                      ((standard 'deep-get) perm
                       '(1 (*crypto* signature))))))))))
          (selected
           '((1 *state* alice leaf)
             (1 *state* alice dir a)
             (1 *state* alice dir b))))

      ;; Configured-admin scalar and internal-journal batch operations succeed.
      ;; The batch contains exact duplicates and an overlapping directory.
      (test-submit ((bob journal-1 'prune!) '(-1 *state* alice leaf))
                   :expect #t)
      (test-submit
       ((*journal* journal-1 'prune-batch!)
        '((-1 *state* alice dir a)
          (-1 *state* alice dir)
          (-1 *state* alice dir a)))
       :expect #t)

      ;; Missing, already-pruned, and duplicate/overlap requests remain #t.
      (test-submit ((bob journal-1 'prune!) '(-1 *state* alice missing))
                   :expect #t)
      (test-submit ((bob journal-1 'prune!) '(-1 *state* alice leaf))
                   :expect #t)
      (test-submit
       ((bob journal-1 'prune-batch!)
        '((-1 *state* alice dir) (-1 *state* alice dir)))
       :expect #t)

      ;; The comparison journal applies only the equivalent union.
      (test-submit
       ((bob journal-2 'prune-batch!)
        '((-1 *state* alice leaf) (-1 *state* alice dir)))
       :expect #t)

      ;; Separate requests reopen persisted state. Both retained fields lose the
      ;; selected leaf/directory while stage, neighbors, and the prior index stay.
      (test-submit (retention-observation journal-1 selected)
                   :expect '(((unknown) (unknown) (unknown))
                             ((unknown) (unknown) (unknown))))
      (test-submit (retention-observation journal-2 selected)
                   :expect '(((unknown) (unknown) (unknown))
                             ((unknown) (unknown) (unknown))))
      (test-submit
       ((*journal* journal-1 'retrieve)
        '(1 *state* alice leaf) :pinned? #f :proof? #f)
       :expect '(unknown))
      (test-submit
       ((*journal* journal-1 'retrieve)
        '(1 *state* alice dir a) :pinned? #f :proof? #f)
       :expect '(unknown))
      (test-submit
       ((*journal* journal-1 'retrieve)
        '(1 *state* alice keep) :pinned? #f :proof? #f)
       :expect "keep-1")
      (test-submit
       ((*journal* journal-1 'retrieve)
        '(0 *state* alice leaf) :pinned? #f :proof? #f)
       :expect "leaf-0")
      (test-submit ((*journal* journal-1 'use!) '(*state* alice leaf))
                   :expect "staged")
      (test-submit ((*journal* journal-1 'use!) '(*state* alice stage-only))
                   :expect "keep")

      ;; Digest, signed-root identity, chain size/numbering, and stage are exact.
      (test-submit
       (raw journal-1
            '(*call* "pass-1"
                     (lambda (root)
              (let* ((ledger (sync-eval ((root 'get) '(root object ledger))))
                     (standard (sync-eval ((ledger '~field!) 'standard)))
                     (perm ((ledger '~field!) 'perm)))
                (list
                 (sync-digest (root))
                 (sync-digest (ledger))
                 (sync-digest ((ledger '~field!) 'stage))
                 (sync-digest perm)
                 (sync-digest ((ledger '~field!) 'temp))
                 ((ledger 'size))
                 ((standard 'deep-get) perm
                  '(1 (*crypto* signature))))))))
       :expect before))

    ;; A later commit retains the original next index, proving numbering and
    ;; append behavior were unchanged by committed-retention pruning.
    (test-submit ((*journal* journal-1 'step!)) :expect 3)
    (test-submit
     ((*journal* journal-1 'retrieve)
      '(2 *state* alice leaf) :pinned? #f :proof? #f)
     :expect "staged")

    (test-report)))
